use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_early_turn_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    matches!(tokens, [first, second, third, fourth, ..]
        if first.raw.eq_ignore_ascii_case("this")
            && second.raw.eq_ignore_ascii_case("land")
            && third.raw.eq_ignore_ascii_case("enters")
            && fourth.raw.eq_ignore_ascii_case("tapped"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("unless"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("turn"))
}

pub(in crate::oracle::canonical) fn parse_promoted_early_turn_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("early-turn-entry", input.tokens);
    let entry = recognize_early_turn_entry(&mut cursor)?;
    let player = controller();
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
            "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
            "condition": not(and(vec![
                json!({
                    "kind": "duringControllerTurn",
                    "player": player.clone(),
                }),
                compare(
                    "<=",
                    json!({
                        "kind": "countTurnsTaken",
                        "player": player,
                    }),
                    integer(entry.maximum_controller_turn),
                ),
            ])),
            "replacement": [{
                "kind": "setEnteringState",
                "object": self_ref(),
                "tapped": true,
            }],
        }),
        &[
            "Recognize the controller's consecutive early-turn ordinal list",
            "Lower turn counting and comparison as independent primitive expressions",
            "Apply one atomic tapped entering-state replacement outside that interval",
            &trace,
        ],
    ))
}
