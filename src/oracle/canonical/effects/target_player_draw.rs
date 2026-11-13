use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_target_player_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    matches!(input.tokens, [target, player, action, _, _, terminator]
        if target.raw.eq_ignore_ascii_case("target")
            && player.raw.eq_ignore_ascii_case("player")
            && action.raw.eq_ignore_ascii_case("draws")
            && terminator.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_target_player_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("target-player-draw", input.tokens);
    let node = recognize_target_player_draw(&mut cursor)?;
    cursor.enter("target-player-draw.validation");
    cursor.require(
        matches!(node.target.slots.as_slice(), [slot]
            if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
                && slot.cardinality == TargetCardinality::EXACTLY_ONE),
        "exactly one target player",
    )?;
    let count = lower_numeric_atom(node.count);
    let (target_id, candidates, minimum, maximum) = lower_single_target(&node.target);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": target_id,
                    "kind": "chooseTargets",
                    "minimum": minimum,
                    "maximum": maximum,
                    "candidates": candidates,
                }],
            },
            "effects": [{
                "kind": "drawCards",
                "player": chosen_target(target_id),
                "count": count,
            }],
        }),
        &[
            "Bind one target player at casting declaration",
            "Recognize the draw count through the shared numeric child",
            &trace,
        ],
    ))
}
