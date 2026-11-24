use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, NumericAtom, NumericExpression, NumericObjectProperty};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourcePropertyState {
    SourceReference,
    PossessiveObject,
    Property,
    Complete,
}

/// Recognizes a numeric property of the current ability source. The object
/// noun is lexical evidence for the self-reference, not a new runtime object
/// domain, so all accepted surfaces lower through `LocalObjectReference`.
pub(in crate::oracle::canonical) fn recognize_source_object_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let mut state = SourcePropertyState::SourceReference;
    let mut property = None;
    loop {
        state = match state {
            SourcePropertyState::SourceReference => {
                cursor.enter("numeric.object-property.source-reference");
                let reference = cursor.consume_type("referenceWord")?;
                cursor.require(
                    reference.raw.eq_ignore_ascii_case("this"),
                    "the current source reference `this`",
                )?;
                SourcePropertyState::PossessiveObject
            }
            SourcePropertyState::PossessiveObject => {
                cursor.enter("numeric.object-property.possessive-object");
                let object = cursor.consume_type("possessiveTypeWord")?;
                cursor.require(
                    registered_possessive_object(&object.raw),
                    "a registered possessive card type or object domain",
                )?;
                SourcePropertyState::Property
            }
            SourcePropertyState::Property => {
                cursor.enter("numeric.object-property.property");
                let token = cursor.consume_type("propertyWord")?;
                property = Some(if token.raw.eq_ignore_ascii_case("power") {
                    NumericObjectProperty::Power
                } else if token.raw.eq_ignore_ascii_case("toughness") {
                    NumericObjectProperty::Toughness
                } else {
                    return Err(cursor.fail("`power` or `toughness`"));
                });
                SourcePropertyState::Complete
            }
            SourcePropertyState::Complete => {
                cursor.enter("numeric.object-property.complete");
                return Ok(NumericExpression::ObjectProperty {
                    object: LocalObjectReference::SourceSelf,
                    property: cursor
                        .require_value(property, "a populated numeric object property")?,
                });
            }
        };
    }
}

fn object_property_word(raw: &str) -> Option<NumericObjectProperty> {
    if raw.eq_ignore_ascii_case("power") {
        Some(NumericObjectProperty::Power)
    } else if raw.eq_ignore_ascii_case("toughness") {
        Some(NumericObjectProperty::Toughness)
    } else {
        None
    }
}

/// Lexical lookahead for `its power|toughness`; the parent decides whether a
/// subject antecedent exists before the child is allowed to read it.
pub(in crate::oracle::canonical) fn subject_possessive_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| object_property_word(raw).is_some())
}

/// `its power|toughness` bound to the subject supplied by the enclosing action
/// frame. The pronoun never mints a reference: without a parent subject the
/// arithmetic grammar does not offer this child at all.
pub(in crate::oracle::canonical) fn recognize_subject_possessive_property(
    cursor: &mut OracleTokenCursor<'_>,
    subject: LocalObjectReference,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.subject-possessive.pronoun");
    cursor.consume("its")?;
    cursor.enter("numeric.subject-possessive.property");
    let property = cursor
        .peek()
        .and_then(object_property_word)
        .ok_or_else(|| cursor.fail("`power` or `toughness`"))?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    Ok(NumericExpression::ObjectProperty {
        object: subject,
        property,
    })
}

/// `its power|toughness|mana value` outside an action frame that supplies a
/// subject (`create X tokens, where X is its power`): the possessive names
/// the object the binding environment binds to a bare `it`. A lexical
/// antecedent of the program wins; otherwise the event's grammatical
/// subject (`When enchanted creature dies`), whose last known information
/// answers the read after it left (CR 608.2h). Without either the reader
/// fails; the pronoun never mints a reference.
pub(in crate::oracle::canonical) fn environment_possessive_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("power")
                || raw.eq_ignore_ascii_case("toughness")
                || (raw.eq_ignore_ascii_case("mana")
                    && cursor
                        .peek_ahead(2)
                        .is_some_and(|next| next.eq_ignore_ascii_case("value")))
        })
}

pub(in crate::oracle::canonical) fn recognize_environment_possessive_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &super::BindingEnvironment,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.possessive.pronoun");
    cursor.consume("its")?;
    cursor.enter("numeric.possessive.property");
    let property = super::recognize_selector_numeric_property(cursor)?;
    cursor.enter("numeric.possessive.antecedent");
    // A lexical antecedent of the program (a declared target) keeps the
    // declared-slot property, validated against its selector on lowering.
    if let Some(antecedent) = environment.object.as_ref()
        && let super::PermanentEffectReference::Target(binding) = antecedent.reference
    {
        return Ok(match property {
            super::SelectorNumericProperty::Power => NumericExpression::PowerOf(binding),
            super::SelectorNumericProperty::ManaValue => NumericExpression::ManaValueOf(binding),
            super::SelectorNumericProperty::Toughness => {
                return Err(cursor.fail("a declared-target toughness read with its nominal"));
            }
        });
    }
    let slot = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| slot.subject && environment.object.is_none())
        .ok_or_else(|| cursor.fail("an event subject or a preceding target that `its` names"))?;
    Ok(NumericExpression::BoundObjectProperty {
        object: slot.reference,
        property,
    })
}

/// Lexical lookahead for `the sacrificed NOUN's ...`.
pub(in crate::oracle::canonical) fn sacrificed_cost_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("sacrificed"))
}

/// `the sacrificed NOUN's power|toughness`: a characteristic of the permanent
/// sacrificed to pay a cost of this ability (CR 601.2h, 602.2b), read as last
/// known information when the cost was paid (CR 608.2h). The symbolic
/// reference is resolved against a same-face cost producer after card
/// composition; it never borrows another object.
pub(in crate::oracle::canonical) fn recognize_sacrificed_cost_object_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.sacrificed-cost-object.reference");
    cursor.consume_phrase(&["the", "sacrificed"])?;
    cursor.enter("numeric.sacrificed-cost-object.possessive-object");
    let object = cursor.consume_type("possessiveTypeWord")?;
    cursor.require(
        registered_possessive_object(&object.raw),
        "a registered possessive card type or object domain",
    )?;
    cursor.enter("numeric.sacrificed-cost-object.property");
    let token = cursor.consume_type("propertyWord")?;
    let property =
        object_property_word(&token.raw).ok_or_else(|| cursor.fail("`power` or `toughness`"))?;
    Ok(NumericExpression::SacrificedCostObjectProperty(property))
}

fn registered_possessive_object(raw: &str) -> bool {
    let Some(base) = raw.strip_suffix("'s").or_else(|| raw.strip_suffix("’s")) else {
        return false;
    };
    matches!(
        lookup_object_selector_lexeme(base),
        Some(CharacteristicLexeme::CardType(_) | CharacteristicLexeme::ObjectDomain(_))
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VariableDefinitionState {
    Where,
    Variable,
    Copula,
    Value,
    Complete,
}

/// Recognizes a source-property variable definition such as
/// `where X is this creature's power`. The parent supplies the variable it
/// expects to bind, so this child cannot silently bind a different symbol.
pub(in crate::oracle::canonical) fn recognize_source_property_variable_definition(
    cursor: &mut OracleTokenCursor<'_>,
    expected_variable: NumericAtom,
) -> MachineResult<NumericExpression> {
    let mut state = VariableDefinitionState::Where;
    let mut expression = None;
    loop {
        state = match state {
            VariableDefinitionState::Where => {
                cursor.enter("numeric.variable-definition.where");
                cursor.consume("where")?;
                VariableDefinitionState::Variable
            }
            VariableDefinitionState::Variable => {
                cursor.enter("numeric.variable-definition.variable");
                cursor.require(
                    expected_variable == NumericAtom::VariableX,
                    "the X variable binding requested by the parent",
                )?;
                let variable = cursor.consume_type("quantity")?;
                cursor.require(variable.raw.eq_ignore_ascii_case("x"), "the variable X")?;
                VariableDefinitionState::Copula
            }
            VariableDefinitionState::Copula => {
                cursor.enter("numeric.variable-definition.copula");
                cursor.consume("is")?;
                VariableDefinitionState::Value
            }
            VariableDefinitionState::Value => {
                cursor.enter("numeric.variable-definition.value");
                expression = Some(recognize_source_object_property(cursor)?);
                VariableDefinitionState::Complete
            }
            VariableDefinitionState::Complete => {
                cursor.enter("numeric.variable-definition.complete");
                return cursor.require_value(expression, "a source-property value for X");
            }
        };
    }
}

#[cfg(test)]
#[path = "numeric_object_property_tests.rs"]
mod tests;

pub(in crate::oracle::canonical) fn source_counter_count_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["the", "number", "of"].iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    }) && cursor.peek_ahead(3).is_some()
        && cursor
            .token_slice(cursor.position() + 3, cursor.position() + 4)
            .first()
            .is_some_and(|token| {
                matches!(
                    token.token_type.as_str(),
                    "counterNamePart" | "counterValue"
                )
            })
}

/// Pure source query. Counter names remain data from the shared bounded slot.
/// The caller supplies source availability; no event/pronoun binding is minted.
pub(in crate::oracle::canonical) fn recognize_source_counter_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.source-counter.quantity");
    cursor.consume_phrase(&["the", "number", "of"])?;
    recognize_source_counter_operand(cursor)
}

/// `COUNTER-NAME counter(s) on this NOUN`: the counted operand shared by the
/// `the number of` quantity and the `for each` multiplier.
pub(in crate::oracle::canonical) fn recognize_source_counter_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let counter = super::recognize_counter_name(cursor)?;
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("counter or counters"))?
        .to_owned();
    cursor.consume(&noun)?;
    cursor.enter("numeric.source-counter.source");
    cursor.consume_phrase(&["on", "this"])?;
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a registered source object noun"))?
        .to_owned();
    cursor.require(
        match lookup_object_selector_lexeme(&noun) {
            Some(CharacteristicLexeme::CardType(kind)) => kind.may_appear_on_permanent(),
            Some(CharacteristicLexeme::ObjectDomain(
                crate::oracle::characteristic_vocabulary::ObjectDomain::Permanent
                | crate::oracle::characteristic_vocabulary::ObjectDomain::Card,
            )) => true,
            _ => false,
        },
        "a source noun compatible with the proved battlefield information context",
    )?;
    cursor.consume(&noun)?;
    Ok(NumericExpression::CountCounters {
        object: LocalObjectReference::SourceSelf,
        counter,
    })
}
