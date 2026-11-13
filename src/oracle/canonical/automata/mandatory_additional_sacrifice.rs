use super::super::{MachineFailure, OracleTokenCursor};
use super::source_sacrifice::atomic_action::recognize_sacrifice_action;
use super::{
    ObjectKind, ObjectQueryNode, object_predicate_uses_x,
    recognize_mandatory_additional_cost_header, recognize_object_query,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MandatoryAdditionalSacrificeCostNode {
    pub object: ObjectQueryNode,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) enum MandatoryAdditionalSacrificeCostAttempt {
    Parsed(MandatoryAdditionalSacrificeCostNode),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MandatoryAdditionalSacrificeState {
    Envelope,
    Action,
    Terminator,
    Complete,
}

/// Recognizes the mandatory casting-cost envelope and delegates its payment
/// action to the shared sacrifice head and object-query grammar. The parent
/// owns neither card types nor Boolean selector composition.
/// Unsupported query grammar and contextual producers are tentative. An
/// impossible sacrifice domain is malformed, while successful recognition
/// commits one typed operand to the mandatory-cost envelope.
pub(in crate::oracle::canonical) fn try_recognize_mandatory_additional_sacrifice_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MandatoryAdditionalSacrificeCostAttempt {
    let checkpoint = cursor.checkpoint();
    let mut state = MandatoryAdditionalSacrificeState::Envelope;
    let mut object = None;

    loop {
        state = match state {
            MandatoryAdditionalSacrificeState::Envelope => {
                cursor.enter("additional-cost.mandatory-sacrifice.envelope");
                if let Err(failure) = recognize_mandatory_additional_cost_header(cursor) {
                    return MandatoryAdditionalSacrificeCostAttempt::Failed(failure);
                }
                MandatoryAdditionalSacrificeState::Action
            }
            MandatoryAdditionalSacrificeState::Action => {
                cursor.enter("additional-cost.mandatory-sacrifice.action");
                let action = match recognize_sacrifice_action(cursor, |cursor| {
                    cursor.enter("additional-cost.mandatory-sacrifice.article");
                    let article = cursor
                        .peek()
                        .filter(|raw| {
                            raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                        })
                        .ok_or_else(|| cursor.fail("`a` or `an` before the sacrificed permanent"))?
                        .to_string();
                    cursor.consume(&article)?;

                    cursor.enter("additional-cost.mandatory-sacrifice.object-query");
                    recognize_object_query(cursor)
                }) {
                    Ok(action) => action,
                    Err(failure) => {
                        cursor.rollback(checkpoint);
                        return MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure);
                    }
                };
                cursor.enter("additional-cost.mandatory-sacrifice.object-query");
                if let Err(failure) = cursor.require(
                    action.operand.controller.is_none(),
                    "an implicit permanent controlled by the spell's caster",
                ) {
                    cursor.rollback(checkpoint);
                    return MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure);
                }
                // The sacrifice action supplies the battlefield domain. A
                // subtype noun is sufficient; a bare adjective is not.
                if let Err(failure) = cursor.require(
                    action
                        .operand
                        .predicate
                        .supports_domain(ObjectKind::Permanent)
                        && action.operand.predicate.has_nominal_head(),
                    "a permanent object predicate with a nominal head",
                ) {
                    return MandatoryAdditionalSacrificeCostAttempt::Failed(failure);
                }
                cursor.enter("additional-cost.mandatory-sacrifice.numeric-context");
                if let Err(failure) = cursor.require(
                    !object_predicate_uses_x(&action.operand.predicate),
                    "contextual numeric producer support before using X in an additional cost",
                ) {
                    cursor.rollback(checkpoint);
                    return MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure);
                }
                object = Some(action.operand);
                MandatoryAdditionalSacrificeState::Terminator
            }
            MandatoryAdditionalSacrificeState::Terminator => {
                cursor.enter("additional-cost.mandatory-sacrifice.terminator");
                if let Err(failure) = cursor.consume(".") {
                    cursor.rollback(checkpoint);
                    return MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure);
                }
                MandatoryAdditionalSacrificeState::Complete
            }
            MandatoryAdditionalSacrificeState::Complete => {
                cursor.enter("additional-cost.mandatory-sacrifice.complete");
                if let Err(failure) = cursor.finish() {
                    return MandatoryAdditionalSacrificeCostAttempt::Failed(failure);
                }
                return match cursor.require_value(object, "a populated sacrifice object query") {
                    Ok(object) => MandatoryAdditionalSacrificeCostAttempt::Parsed(
                        MandatoryAdditionalSacrificeCostNode { object },
                    ),
                    Err(failure) => MandatoryAdditionalSacrificeCostAttempt::Failed(failure),
                };
            }
        };
    }
}

#[cfg(test)]
#[path = "mandatory_additional_sacrifice_tests.rs"]
mod tests;
