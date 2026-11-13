use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AddAnyColorManaEffectNode, AddFixedManaEffectNode, AddManaChoiceEffectNode, AddManaEffectNode,
    AddScopedManaEffectNode, ManaChoiceDomain, NumericAtom, ObjectCardType, ObjectControllerScope,
    ObjectPredicate, recognize_fixed_mana_sequence, recognize_numeric_atom, recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddFixedManaChoiceState {
    Action,
    Option,
    ConnectorOrComplete,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManaChoiceConnector {
    Comma,
    Or,
    CommaOr,
}

/// Recognizes one or more alternative fixed-mana outputs. `or` belongs to
/// this mana-choice parent rather than to an effect-sequence or Boolean
/// grammar; every branch remains an independently parsed fixed mana sequence.
fn recognize_add_fixed_mana_or_choice_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddManaEffectNode> {
    let mut state = AddFixedManaChoiceState::Action;
    let mut options = Vec::new();
    let mut connectors = Vec::new();
    loop {
        state = match state {
            AddFixedManaChoiceState::Action => {
                cursor.enter("effect.add-fixed-mana-choice.action");
                cursor.consume("add")?;
                AddFixedManaChoiceState::Option
            }
            AddFixedManaChoiceState::Option => {
                cursor.enter("effect.add-fixed-mana-choice.option");
                options.push(recognize_fixed_mana_sequence(cursor)?);
                AddFixedManaChoiceState::ConnectorOrComplete
            }
            AddFixedManaChoiceState::ConnectorOrComplete => {
                cursor.enter("effect.add-fixed-mana-choice.connector-or-complete");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                    {
                        cursor.consume("or")?;
                        connectors.push(ManaChoiceConnector::CommaOr);
                    } else {
                        connectors.push(ManaChoiceConnector::Comma);
                    }
                    AddFixedManaChoiceState::Option
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    connectors.push(ManaChoiceConnector::Or);
                    AddFixedManaChoiceState::Option
                } else {
                    AddFixedManaChoiceState::Complete
                }
            }
            AddFixedManaChoiceState::Complete => {
                cursor.enter("effect.add-fixed-mana-choice.complete");
                let valid_connectors = match options.len() {
                    0 => false,
                    1 => connectors.is_empty(),
                    2 => connectors == [ManaChoiceConnector::Or],
                    _ => {
                        connectors.len() + 1 == options.len()
                            && connectors.split_last().is_some_and(|(last, prefix)| {
                                *last == ManaChoiceConnector::CommaOr
                                    && prefix
                                        .iter()
                                        .all(|connector| *connector == ManaChoiceConnector::Comma)
                            })
                    }
                };
                cursor.require(valid_connectors, "a canonical fixed-mana choice list")?;
                return match options.len() {
                    0 => Err(cursor.fail("one or more fixed-mana output options")),
                    1 => Ok(AddManaEffectNode::Fixed(AddFixedManaEffectNode {
                        mana: options.pop().expect("one fixed-mana option"),
                    })),
                    _ => Ok(AddManaEffectNode::Choice(AddManaChoiceEffectNode {
                        options,
                    })),
                };
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddAnyColorManaState {
    Action,
    Amount,
    ManaNoun,
    Of,
    Any,
    Color,
    Complete,
}

/// Recognizes unrestricted `Add <literal> mana of any color` production.
/// Numeric amount and color vocabulary stay owned by their shared children;
/// spending restrictions and scoped color domains belong to separate parents.
pub(in crate::oracle::canonical) fn recognize_add_any_color_mana_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddAnyColorManaEffectNode> {
    let mut state = AddAnyColorManaState::Action;
    let mut amount = None;
    loop {
        state = match state {
            AddAnyColorManaState::Action => {
                cursor.enter("effect.add-any-color-mana.action");
                cursor.consume("add")?;
                AddAnyColorManaState::Amount
            }
            AddAnyColorManaState::Amount => {
                cursor.enter("effect.add-any-color-mana.amount");
                amount = match recognize_numeric_atom(cursor)? {
                    NumericAtom::Integer(value) if value > 0 => Some(value),
                    _ => return Err(cursor.fail("a positive literal mana amount")),
                };
                AddAnyColorManaState::ManaNoun
            }
            AddAnyColorManaState::ManaNoun => {
                cursor.enter("effect.add-any-color-mana.mana-noun");
                let token = cursor.consume_type("costWord")?;
                cursor.require(token.raw.eq_ignore_ascii_case("mana"), "the mana noun")?;
                AddAnyColorManaState::Of
            }
            AddAnyColorManaState::Of => {
                cursor.enter("effect.add-any-color-mana.of");
                cursor.consume("of")?;
                AddAnyColorManaState::Any
            }
            AddAnyColorManaState::Any => {
                cursor.enter("effect.add-any-color-mana.any");
                cursor.consume("any")?;
                AddAnyColorManaState::Color
            }
            AddAnyColorManaState::Color => {
                cursor.enter("effect.add-any-color-mana.color");
                cursor.consume_type("colorNoun")?;
                AddAnyColorManaState::Complete
            }
            AddAnyColorManaState::Complete => {
                cursor.enter("effect.add-any-color-mana.complete");
                return Ok(AddAnyColorManaEffectNode {
                    amount: cursor.require_value(amount, "a populated mana amount")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LandColorDomainState {
    Relative,
    Determiner,
    LandSelector,
    Capability,
    Production,
    Complete,
}

/// Recognizes the relative color domain `that a land CONTROLLER could
/// produce`. Land identity and controller relation are delegated to the same
/// object-query child used by targeting and set grammars.
fn recognize_land_color_domain(
    cursor: &mut OracleTokenCursor<'_>,
    amount: i64,
) -> MachineResult<AddScopedManaEffectNode> {
    let mut state = LandColorDomainState::Relative;
    let mut controller = None;
    loop {
        state = match state {
            LandColorDomainState::Relative => {
                cursor.enter("effect.add-land-color-domain.relative");
                cursor.consume("that")?;
                LandColorDomainState::Determiner
            }
            LandColorDomainState::Determiner => {
                cursor.enter("effect.add-land-color-domain.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an"),
                    "an indefinite object determiner",
                )?;
                LandColorDomainState::LandSelector
            }
            LandColorDomainState::LandSelector => {
                cursor.enter("effect.add-land-color-domain.land-selector");
                let query = recognize_object_query(cursor)?;
                cursor.enter("effect.add-land-color-domain.land-selector.validation");
                cursor.require(
                    matches!(
                        query.predicate,
                        ObjectPredicate::CardType(ObjectCardType::Land)
                    ),
                    "the Land card type as the color-producing object",
                )?;
                cursor.enter("effect.add-land-color-domain.controller-scope");
                cursor.require(
                    query.controller == Some(ObjectControllerScope::OpponentOfAbilityController),
                    "lands controlled by an opponent of the ability controller",
                )?;
                controller = query.controller;
                LandColorDomainState::Capability
            }
            LandColorDomainState::Capability => {
                cursor.enter("effect.add-land-color-domain.capability");
                let modal = cursor.consume_type("modalWord")?;
                cursor.require(modal.raw.eq_ignore_ascii_case("could"), "the `could` modal")?;
                LandColorDomainState::Production
            }
            LandColorDomainState::Production => {
                cursor.enter("effect.add-land-color-domain.production");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("produce"),
                    "the `produce` mana action",
                )?;
                LandColorDomainState::Complete
            }
            LandColorDomainState::Complete => {
                cursor.enter("effect.add-land-color-domain.complete");
                return Ok(AddScopedManaEffectNode {
                    amount,
                    domain: ManaChoiceDomain::LandColorsControlledBy(
                        cursor.require_value(controller, "a populated land-controller relation")?,
                    ),
                });
            }
        };
    }
}

/// Dispatches between independent fixed-symbol and unrestricted color-choice
/// mana-production children from the shared token cursor.
pub(in crate::oracle::canonical) fn recognize_add_mana_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddManaEffectNode> {
    cursor.enter("effect.add-mana.dispatch");
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.starts_with('{') && raw.ends_with('}'))
    {
        return recognize_add_fixed_mana_or_choice_effect(cursor);
    }
    let any_color = recognize_add_any_color_mana_effect(cursor)?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        return recognize_land_color_domain(cursor, any_color.amount)
            .map(AddManaEffectNode::ScopedColor);
    }
    Ok(AddManaEffectNode::AnyColor(any_color))
}

#[cfg(test)]
#[path = "mana_tests.rs"]
mod tests;
