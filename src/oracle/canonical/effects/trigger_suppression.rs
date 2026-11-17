use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims an entering-event trigger-suppression family by token structure.
/// The local FSM owns the selector, optional battlefield phrase, and tail.
pub(in crate::oracle::canonical) fn claims_promoted_trigger_suppression_family(
    input: &AbilityInput<'_>,
) -> bool {
    let Some(entering_index) = input.tokens.iter().position(|token| {
        token.token_type == "eventWord" && token.raw.eq_ignore_ascii_case("entering")
    }) else {
        return false;
    };
    input
        .tokens
        .first()
        .is_some_and(|token| token.token_type == "typeWord")
        && input.tokens[entering_index + 1..]
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("don't"))
}

pub(in crate::oracle::canonical) fn parse_promoted_trigger_suppression(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("trigger-creation-suppression", input.tokens);
    let node = recognize_trigger_creation_suppression(&mut cursor)?;
    let caused_by = match node.caused_by {
        TriggerCauseEventNode::EnterBattlefield { object } => json!({
            "kind": "enterBattlefield",
            "object": {
                "kind": "eventObject",
                "where": lower_permanent_object_predicate(&object.predicate),
            },
        }),
    };
    let transition_trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "suppressTriggerCreation",
                "causedBy": caused_by,
            }],
        }),
        &[
            "Resolve the entering-object collection through the shared selector automaton",
            "Model entering as an event predicate",
            "Install one primitive trigger-creation suppression modifier",
            &transition_trace,
        ],
    ))
}
