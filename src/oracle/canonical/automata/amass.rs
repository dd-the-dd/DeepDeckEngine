use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_amass_army_subtype};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AmassEffectNode, LocalPlayerReference, NumericAtom, NumericExpression, TargetBinding,
    recognize_numeric_expression,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmassState {
    Action,
    ArmySubtype,
    Count,
    Complete,
}

/// Recognizes the procedural keyword action `amass TYPE COUNT`.
///
/// The subtype must come from the closed amass vocabulary; the machine never
/// singularizes an unknown word or treats UNKNOWN as a creature subtype.
pub(in crate::oracle::canonical) fn recognize_amass_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<AmassEffectNode> {
    let mut state = AmassState::Action;
    let mut army_subtype = None;
    let mut count = None;
    loop {
        state = match state {
            AmassState::Action => {
                cursor.enter("effect.amass.action");
                cursor.consume("amass")?;
                AmassState::ArmySubtype
            }
            AmassState::ArmySubtype => {
                cursor.enter("effect.amass.army-subtype");
                let token_index = cursor.position();
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("a plural supported amass subtype"))?;
                let subtype =
                    lookup_amass_army_subtype(&token.raw, AcceptedForms::SingularOrPlural)
                        .filter(|subtype| token.raw.eq_ignore_ascii_case(subtype.oracle_plural()))
                        .ok_or_else(|| {
                            cursor.fail_with_span(
                                "a plural supported amass subtype",
                                token_index,
                                token.span_start,
                                token.span_end,
                                token.raw.clone(),
                            )
                        })?;
                cursor.advance_to(token_index + 1)?;
                army_subtype = Some(subtype);
                AmassState::Count
            }
            AmassState::Count => {
                cursor.enter("effect.amass.count");
                let expression = recognize_numeric_expression(cursor, antecedent)?;
                cursor.require(
                    !matches!(
                        &expression,
                        NumericExpression::Atom(NumericAtom::Integer(value)) if *value < 0
                    ),
                    "a nonnegative amass count or variable expression",
                )?;
                count = Some(expression);
                AmassState::Complete
            }
            AmassState::Complete => {
                cursor.enter("effect.amass.complete");
                return Ok(AmassEffectNode {
                    player: LocalPlayerReference::Controller,
                    army_subtype: cursor
                        .require_value(army_subtype, "a supported amass subtype")?,
                    count: cursor.require_value(count, "an amass count")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "amass_tests.rs"]
mod tests;
