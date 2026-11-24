use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AddAnyColorAmountManaEffectNode, AddAnyColorManaEffectNode, AddCountedManaEffectNode,
    AddFixedManaEffectNode, AddManaChoiceEffectNode, AddManaEffectNode, AddScopedManaEffectNode,
    ManaChoiceDomain, ManaColorDistribution, NumericAtom, NumericExpression, ObjectCardType,
    ObjectControllerScope, ObjectPredicate, PermanentCountMeasure, recognize_fixed_mana_sequence,
    recognize_numeric_atom, recognize_object_query,
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
                // A comma continues this choice list only before another
                // mana option or its final `or`; any other comma is an
                // effect-sequence boundary owned by the enclosing parent.
                let effect_connector = cursor.peek() == Some(",")
                    && !cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("or") || raw.starts_with('{'));
                if effect_connector {
                    AddFixedManaChoiceState::Complete
                } else if cursor.peek().is_some_and(|raw| raw == ",") {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManaOutputDomain {
    Color,
    Type,
}

pub(in crate::oracle::canonical) fn recognize_add_any_color_mana_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddAnyColorManaEffectNode> {
    let (amount, _, distribution) = recognize_add_any_mana_head(cursor, false)?;
    cursor.enter("effect.add-any-color-mana.literal-amount");
    let amount = literal_mana_amount(&amount)
        .ok_or_else(|| cursor.fail("a positive literal mana amount"))?;
    Ok(AddAnyColorManaEffectNode {
        amount,
        distribution,
    })
}

/// The positive literal of a mana amount, when the amount is one.
fn literal_mana_amount(amount: &NumericExpression) -> Option<i64> {
    match amount {
        NumericExpression::Atom(NumericAtom::Integer(value)) if *value > 0 => Some(*value),
        _ => None,
    }
}

/// The amount slot of a color-choice production: a positive literal, the
/// variable X, or the event's `that much`/`that many`, all read by the shared
/// numeric children. A literal stays the positive quantity it always was.
fn recognize_mana_amount(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        return super::recognize_that_many_event_value(cursor, None);
    }
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => Ok(NumericExpression::from(value)),
        NumericAtom::VariableX => Ok(NumericExpression::Atom(NumericAtom::VariableX)),
        NumericAtom::Integer(_) => {
            cursor.enter("effect.add-any-color-mana.positive-amount");
            Err(cursor.fail("a positive mana amount, X, or `that much`"))
        }
    }
}

fn recognize_add_any_mana_head(
    cursor: &mut OracleTokenCursor<'_>,
    allow_types: bool,
) -> MachineResult<(NumericExpression, ManaOutputDomain, ManaColorDistribution)> {
    let mut output_domain = ManaOutputDomain::Color;
    let mut distribution = ManaColorDistribution::OneColor;
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
                amount = Some(recognize_mana_amount(cursor)?);
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
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
                {
                    // `in any combination of colors`: one independent color
                    // choice per mana.
                    cursor.enter("effect.add-any-color-mana.combination");
                    cursor.consume_phrase(&["in", "any", "combination", "of", "colors"])?;
                    distribution = ManaColorDistribution::AnyCombination;
                    AddAnyColorManaState::Complete
                } else {
                    cursor.consume("of")?;
                    AddAnyColorManaState::Any
                }
            }
            AddAnyColorManaState::Any => {
                cursor.enter("effect.add-any-color-mana.any");
                cursor.consume("any")?;
                AddAnyColorManaState::Color
            }
            AddAnyColorManaState::Color => {
                cursor.enter("effect.add-any-color-mana.color");
                if allow_types
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("type"))
                {
                    cursor.consume_type("propertyWord")?;
                    cursor.enter("effect.add-any-color-mana.type-amount");
                    cursor.require(
                        amount
                            .as_ref()
                            .and_then(literal_mana_amount)
                            .is_some_and(|amount| amount <= i64::from(i32::MAX)),
                        "a positive mana type amount representable by the runtime i32 quantity",
                    )?;
                    output_domain = ManaOutputDomain::Type;
                } else {
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("one"))
                    {
                        // `any one color`: the explicit single-color form.
                        cursor.enter("effect.add-any-color-mana.one-color");
                        cursor.consume("one")?;
                        cursor.require(
                            cursor
                                .peek()
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("color")),
                            "the singular `color` after `any one`",
                        )?;
                    }
                    cursor.consume_type("colorNoun")?;
                }
                AddAnyColorManaState::Complete
            }
            AddAnyColorManaState::Complete => {
                cursor.enter("effect.add-any-color-mana.complete");
                return Ok((
                    cursor.require_value(amount, "a populated mana amount")?,
                    output_domain,
                    distribution,
                ));
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
    amount: NumericExpression,
    output_domain: ManaOutputDomain,
) -> MachineResult<AddScopedManaEffectNode> {
    cursor.enter("effect.add-land-color-domain.amount");
    let amount = literal_mana_amount(&amount)
        .ok_or_else(|| cursor.fail("a positive literal amount for a scoped color domain"))?;
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
                    query.controller == Some(match output_domain {
                        ManaOutputDomain::Color => ObjectControllerScope::OpponentOfAbilityController,
                        ManaOutputDomain::Type => ObjectControllerScope::AbilityController,
                    }),
                    "a runtime-supported land-controller relation for the selected color/type domain",
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
                    domain: match output_domain {
                        ManaOutputDomain::Color => {
                            ManaChoiceDomain::LandColorsControlledBy(cursor.require_value(
                                controller,
                                "a populated land-controller relation",
                            )?)
                        }
                        ManaOutputDomain::Type => {
                            ManaChoiceDomain::LandTypesControlledByAbilityController
                        }
                    },
                });
            }
        };
    }
}

/// `Add {S} for each COUNTABLE`: the production keeps its fixed symbol and the
/// shared count child supplies the multiplier. Only counts the mana runtime
/// evaluates with the ability source are admitted.
fn recognize_optional_mana_for_each_count(
    cursor: &mut OracleTokenCursor<'_>,
    production: AddManaEffectNode,
) -> MachineResult<AddManaEffectNode> {
    if !super::for_each_count_starts(cursor) {
        return Ok(production);
    }
    cursor.enter("effect.add-mana.for-each.production");
    let AddManaEffectNode::Fixed(fixed) = &production else {
        return Err(cursor.fail("one fixed mana symbol before a `for each` count"));
    };
    let (symbol, multiplicity) = fixed
        .mana
        .repeated_mana_symbol()
        .ok_or_else(|| cursor.fail("one whole-mana symbol repeated before a `for each` count"))?;
    let count = super::recognize_for_each_count(cursor)?;
    cursor.enter("effect.add-mana.for-each.count");
    cursor.require(
        match &count {
            NumericExpression::PermanentCount(node) => {
                node.measure == PermanentCountMeasure::Objects
            }
            NumericExpression::CountCounters { .. } => true,
            // Only zone counts with a holder every evaluator reads.
            NumericExpression::CountQuery(query) => {
                crate::oracle::canonical::effects::lower_scoped_object_query_amount(query).is_some()
            }
            _ => false,
        },
        "a permanent, zone-card or source-counter count evaluable in a mana production",
    )?;
    let amount = super::scale_by_for_each_count(
        &NumericExpression::from(i64::try_from(multiplicity).unwrap_or(i64::MAX)),
        count,
    )
    .ok_or_else(|| cursor.fail("a positive literal mana multiplicity"))?;
    Ok(AddManaEffectNode::Counted(AddCountedManaEffectNode {
        symbol,
        amount,
    }))
}

/// Dispatches between independent fixed-symbol and unrestricted color-choice
/// mana-production children from the shared token cursor.
fn recognize_add_mana_output(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddManaEffectNode> {
    cursor.enter("effect.add-mana.dispatch");
    if cursor
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("for"))
    {
        return super::recognize_add_each_color_mana(cursor).map(AddManaEffectNode::EachColor);
    }

    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.starts_with('{') && raw.ends_with('}'))
    {
        let production = recognize_add_fixed_mana_or_choice_effect(cursor)?;
        return recognize_optional_mana_for_each_count(cursor, production);
    }
    let (amount, output_domain, distribution) = recognize_add_any_mana_head(cursor, true)?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        cursor.enter("effect.add-mana.scoped-domain.distribution");
        cursor.require(
            distribution == ManaColorDistribution::OneColor,
            "a single-color production before a scoped color domain",
        )?;
        return recognize_land_color_domain(cursor, amount, output_domain)
            .map(AddManaEffectNode::ScopedColor);
    }
    cursor.enter("effect.add-mana.output-domain.validation");
    cursor.require(
        output_domain == ManaOutputDomain::Color,
        "a scoped land query after any type",
    )?;
    if let Some(amount) = literal_mana_amount(&amount) {
        return Ok(AddManaEffectNode::AnyColor(AddAnyColorManaEffectNode {
            amount,
            distribution,
        }));
    }
    recognize_expression_amount_any_color(cursor, amount, distribution)
}

/// `Add X mana of any one color, where X is EXPR` / `add that much mana of
/// any one color`: the same color-choice production whose amount is a shared
/// numeric expression. A postposed definition right after the production
/// defines its X (CR 107.3); the production is then the only action that
/// definition can bind. The amount must be one the mana runtime evaluates.
fn recognize_expression_amount_any_color(
    cursor: &mut OracleTokenCursor<'_>,
    amount: NumericExpression,
    distribution: ManaColorDistribution,
) -> MachineResult<AddManaEffectNode> {
    let amount = if amount == NumericExpression::Atom(NumericAtom::VariableX)
        && super::where_x_definition_starts(cursor)
    {
        super::recognize_where_x_definition(cursor)?
    } else {
        amount
    };
    cursor.enter("effect.add-mana.expression-amount.runtime");
    cursor.require(
        crate::oracle::canonical::effects::lower_numeric_expression(&amount).is_some(),
        "a mana amount the shared numeric lowering represents",
    )?;
    Ok(AddManaEffectNode::AnyColorAmount(
        AddAnyColorAmountManaEffectNode {
            amount,
            distribution,
        },
    ))
}

/// A spending qualifier belongs to this producer, not another executed action.
/// The parent still owns the final terminator and complete ability envelope.
pub(in crate::oracle::canonical) fn recognize_add_mana_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddManaEffectNode> {
    let production = recognize_add_mana_output(cursor)?;
    let mut trial = cursor.clone();
    if trial.peek() == Some(".") {
        trial.consume(".")?;
    }
    if !super::mana_spending_policy_starts(&trial) {
        return Ok(production);
    }
    let policy = super::recognize_mana_spending_policy(&mut trial)?;
    *cursor = trial;
    Ok(AddManaEffectNode::Qualified {
        production: Box::new(production),
        policy,
    })
}

#[cfg(test)]
#[path = "mana_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mana_connector_boundary_tests.rs"]
mod connector_boundary_tests;

#[cfg(test)]
#[path = "mana_type_reference_tests.rs"]
mod mana_type_reference_tests;
