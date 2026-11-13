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
