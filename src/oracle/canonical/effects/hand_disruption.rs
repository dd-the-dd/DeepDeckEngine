use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a targeted-hand interaction core, with or without later effects.
/// The claim is deliberately structural: child automata remain responsible
/// for reveal, filtered selection, anaphora, discard, and any suffix.
pub(in crate::oracle::canonical) fn claims_promoted_targeted_hand_disruption_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let boundaries = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (token.raw == ".").then_some(index))
        .collect::<Vec<_>>();
    let [first, second, ..] = boundaries.as_slice() else {
        return false;
    };

    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("player") || token.raw.eq_ignore_ascii_case("opponent")
        })
        && tokens
            .get(first + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(second + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("that"))
}

pub(in crate::oracle::canonical) fn parse_promoted_targeted_hand_disruption(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("targeted-hand-disruption", input.tokens);
    let sequence = recognize_targeted_hand_disruption(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    let TargetedHandDisruptionNode {
        reveal,
        choice,
        discard,
        followup,
    } = sequence;
    cursor.enter("hand-disruption.bindings.validation");
    cursor.require(
        choice.hand_owner == reveal.player && discard.player == reveal.player,
        "one targeted player shared by reveal, hand selection, and discard",
    )?;
    cursor.require(
        discard.card == choice.selection,
        "the previously selected hand card",
    )?;

    let target_candidates = match reveal.target {
        PlayerPredicate::Any => json!({ "kind": "players" }),
        PlayerPredicate::Opponent => json!({
            "kind": "players",
            "where": { "kind": "isOpponentOf", "player": controller() },
        }),
    };
    let target_player = chosen_target("targetPlayer");
    let mut effects = vec![
        json!({
            "kind": "revealHand",
            "player": target_player.clone(),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }),
        json!({
            "kind": "chooseCards",
            "id": "discardedCard",
            "player": controller(),
            "minimum": integer(1),
            "maximum": integer(1),
            "candidates": {
                "kind": "cards",
                "zone": hand(target_player.clone()),
                "where": lower_card_object_predicate(&choice.filter),
            },
        }),
        json!({
            "kind": "discardCards",
            "player": target_player,
            "cards": decision_result("discardedCard"),
        }),
    ];
    if let Some(followup) = followup {
        for (effect_index, effect) in followup.effects.into_iter().enumerate() {
            cursor.enter("hand-disruption.followup.targetless-lowering");
            effects.extend(
                lower_targetless_local_effects(
                    &effect,
                    TargetlessDeclarationSupport::None,
                    Some(&format!("discardFollowup{effect_index}")),
                )
                .map_err(|error| cursor.fail(error.expectation()))?,
            );
        }
    }
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
                "decisions": [target_decision(
                    "targetPlayer",
                    target_candidates,
                    1,
                    1,
                )],
            },
            "effects": effects,
        }),
        &[
            "Recognize the targeted-player reveal from the shared token stream",
            "Bind the revealed hand through an exact anaphoric reference",
            "Compose positive and negative card-type predicates for one card selection",
            "Resolve `that player` and `that card` to the typed prior bindings",
            "Compose any suffix through the shared local effect sequence",
            "Lower to independent revealHand, chooseCards, discardCards, and targetless primitives",
            &trace,
        ],
    ))
}
