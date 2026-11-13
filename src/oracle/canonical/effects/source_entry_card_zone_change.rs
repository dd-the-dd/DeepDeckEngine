use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) const TARGET_GRAVEYARD_CARD_ID: &str = "targetGraveyardCard";

/// Claims the bounded source-entry/card-zone family while leaving articles,
/// zone validity, punctuation, and full consumption to the committed FSM.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_card_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("when")
    }) && tokens
        .iter()
        .any(|token| token.token_type == "eventWord" && token.raw.eq_ignore_ascii_case("enters"))
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("exile")
                && window[1].raw.eq_ignore_ascii_case("target")
                && window[2].raw.eq_ignore_ascii_case("card")
        })
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("from"))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_card_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-card-zone-change", input.tokens);
    let node = recognize_source_entry_target_card_zone_change(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.source-entry-card-zone-change.source-shape");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the current source entering",
    )?;
    let candidates = match node.effect.source_zone {
        TargetCardSourceZone::AnyGraveyard | TargetCardSourceZone::SingleGraveyard => json!({
            "kind": "cards",
            "zone": { "kind": "anyGraveyard" },
            "where": null,
        }),
        TargetCardSourceZone::ControllerGraveyard => json!({
            "kind": "cards",
            "zone": graveyard(controller()),
            "where": lower_card_object_predicate(&node.effect.predicate),
        }),
    };
    let target = chosen_target(TARGET_GRAVEYARD_CARD_ID);
    let targets = json!({
        "kind": "chosenTargets",
        "id": TARGET_GRAVEYARD_CARD_ID,
    });
    let effect = match node.effect.action {
        CardZoneChangeAction::Exile => json!({
            "kind": "moveCards",
            "cards": targets,
            "to": {
                "kind": "exile",
                "player": { "kind": "ownerOf", "object": target },
                "faceDown": false,
            },
        }),
        CardZoneChangeAction::Move(_) => {
            return Err(cursor.fail("an exile action in the source-entry exile family"));
        }
    };

    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    TARGET_GRAVEYARD_CARD_ID,
                    candidates,
                    1,
                    1,
                )],
            },
            "effects": [effect],
        }),
        &[
            "Recognize the source-entry event independently",
            "Recognize one targeted card move from any graveyard",
            "Move the chosen card to its owner's public exile zone",
            "Do not infer any source link from an ordinary exile instruction",
        ],
    ))
}
