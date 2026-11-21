use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

/// Claims the persistent creature-type choice family at its stable entry
/// envelope. The child automata remain responsible for the permanent domain,
/// choice property, terminator, and trailing-input diagnostics.
pub(in crate::oracle::canonical) fn claims_promoted_chosen_creature_type_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.len() >= 10
        && token_is(input, 0, "as")
        && token_is(input, 1, "this")
        && token_is(input, 3, "enters")
        && token_is(input, 4, ",")
        && token_is(input, 5, "choose")
        && token_is(input, 7, "creature")
        && token_is(input, 8, "type")
}

pub(in crate::oracle::canonical) fn parse_promoted_chosen_creature_type_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("chosen-creature-type-entry", input.tokens);
    let node = recognize_chosen_creature_type_entry(&mut cursor)?;
    cursor.enter("chosen-creature-type-entry.binding-validation");
    cursor.require(
        node.choice.binding == CharacteristicChoiceBinding::CreatureType,
        "a creature-type choice binding",
    )?;
    cursor.require(
        node.choice.player == LocalPlayerReference::Controller,
        "the entering object's controller",
    )?;
    let _entering_object = node.entering_object;
    let decision_id = node.choice.binding.decision_id();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "decisions": [{
                "id": decision_id,
                "kind": "chooseCreatureType",
            }],
            "replacement": [{
                "kind": "storeDecision",
                "decisionId": decision_id,
            }],
        }),
        &[
            "Recognize the entering permanent domain from shared characteristic vocabulary",
            "Bind a persistent creature-type choice",
            "Store the choice through generic decision primitives",
            &trace,
        ],
    ))
}
