use super::super::*;

pub(in crate::oracle::canonical) const LOYALTY_GRAVEYARD_TARGET_ID: &str = "targetGraveyardCard";

pub(in crate::oracle::canonical) fn lower_target_card_move_candidates(
    effect: &TargetCardZoneChangeEffectNode,
) -> Value {
    let zone = match effect.source_zone {
        TargetCardSourceZone::ControllerGraveyard => graveyard(controller()),
        TargetCardSourceZone::AnyGraveyard | TargetCardSourceZone::SingleGraveyard => {
            json!({ "kind": "anyGraveyard" })
        }
    };
    json!({
        "kind": "cards",
        "zone": zone,
        "where": lower_card_object_predicate(&effect.predicate),
    })
}

pub(in crate::oracle::canonical) fn lower_target_card_move_effect(
    effect: &TargetCardZoneChangeEffectNode,
    target_id: &str,
) -> Option<Value> {
    let cards = json!({ "kind": "chosenTargets", "id": target_id });
    Some(match effect.action {
        CardZoneChangeAction::Exile => return None,
        CardZoneChangeAction::Move(CardZoneDestination::ControllerHand) => {
            if effect.source_zone != TargetCardSourceZone::ControllerGraveyard {
                return None;
            }
            json!({
                "kind": "moveCards",
                "cards": cards,
                "from": graveyard(controller()),
                "to": { "kind": "hand", "player": controller() },
            })
        }
        CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop) => {
            if effect.source_zone != TargetCardSourceZone::AnyGraveyard {
                return None;
            }
            json!({
                "kind": "moveCards",
                "cards": cards,
                "to": {
                    "kind": "library",
                    "player": {
                        "kind": "ownerOf",
                        "object": chosen_target(target_id),
                    },
                    "position": "top",
                },
            })
        }
    })
}

pub(in crate::oracle::canonical) fn lower_return_target_card_declaration(
    effect: &ReturnTargetCardNode,
) -> Value {
    debug_assert_eq!(effect.owner, LocalPlayerReference::Controller);
    debug_assert_eq!(effect.from, OwnedCardZoneNode::Graveyard);
    target_decision(
        LOYALTY_GRAVEYARD_TARGET_ID,
        json!({
            "kind": "cards",
            "zone": graveyard(controller()),
            "where": lower_card_object_predicate(&effect.predicate),
        }),
        1,
        1,
    )
}

pub(in crate::oracle::canonical) fn lower_return_target_card_effect(
    effect: &ReturnTargetCardNode,
) -> Value {
    debug_assert_eq!(effect.to, OwnedCardZoneNode::Hand);
    json!({
        "kind": "moveCards",
        "cards": {
            "kind": "chosenTargets",
            "id": LOYALTY_GRAVEYARD_TARGET_ID,
        },
        "to": {
            "kind": "hand",
            "player": controller(),
        },
    })
}
