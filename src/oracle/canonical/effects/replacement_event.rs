//! Lowering of event replacements read after `would` (CR 614.1a).
//!
//! A replaced draw whose program is `draw N cards` is the static draw-count
//! replacement of the shared runtime: each replaced draw becomes N draws,
//! excepting the first draw of the controller's draw step when stated.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn try_parse_event_replacement(
    input: &AbilityInput<'_>,
) -> ReplacementFrameAttempt {
    let mut cursor = OracleTokenCursor::new("event-replacement", input.tokens);
    let node = match try_recognize_event_replacement(&mut cursor) {
        ChildOutcome::Matched(node) => node,
        ChildOutcome::Committed(failure) => return ReplacementFrameAttempt::Committed(failure),
        ChildOutcome::NoMatch(_) => return ReplacementFrameAttempt::NotClaimed,
    };
    match lower_event_replacement(&mut cursor, input, &node) {
        Ok(draft) => ReplacementFrameAttempt::Parsed(draft),
        Err(failure) => ReplacementFrameAttempt::Committed(failure),
    }
}

fn lower_event_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    node: &EventReplacementNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("replacement.event.validation");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active replacement",
    )?;
    let modifier = match (&node.event, &node.program) {
        (
            ReplacedEventNode::Draw {
                player,
                except_first_in_draw_step,
            },
            ReplacementProgramNode::Draw { count },
        ) => {
            cursor.require(
                *player == LocalPlayerReference::Controller,
                "the ability controller's draws",
            )?;
            json!({
                "kind": "increaseEachDrawCount",
                "player": controller(),
                "amount": count - 1,
                "exceptFirstInDrawStep": except_first_in_draw_step,
            })
        }
    };
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Recognize the replaced event with the shared subject and verb lexicon",
            "Read the replacement program performed instead",
            "Install one battlefield-active event replacement modifier",
        ],
    ))
}

#[cfg(test)]
#[path = "replacement_event_tests.rs"]
mod tests;
