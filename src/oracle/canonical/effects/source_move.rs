use super::super::*;

/// Lowers one independently recognized source move. The ability envelope owns
/// cross-effect bindings, so the same primitive remains useful outside of
/// transform sequences.
pub(in crate::oracle::canonical) fn lower_source_move_effect(
    effect: &MoveSourceEffectNode,
    cards: Value,
    bind: Option<&str>,
) -> Value {
    // The origin is emitted only when the node proves one; the battlefield and
    // exile moves keep their established origin-free shape.
    let origin = match (effect.from, effect.to) {
        (SourceZone::Graveyard, _) => Some(json!({ "kind": "graveyard", "player": controller() })),
        (SourceZone::Hand, _) => Some(json!({ "kind": "hand", "player": controller() })),
        (SourceZone::Battlefield | SourceZone::Exile | SourceZone::Library, _) => None,
    };
    let mut primitive = match effect.to {
        // A permanent leaves the battlefield through the permanent-movement
        // primitive, which owns its zone-specific lifecycle (CR 400.7).
        SourceZone::Hand if effect.controller == SourceMoveController::Owner => json!({
            "kind": "returnToOwnersHand",
            "object": cards,
        }),
        SourceZone::Hand => json!({
            "kind": "moveCards",
            "cards": cards,
            "to": { "kind": "hand", "player": controller() },
        }),
        SourceZone::Graveyard => json!({
            "kind": "moveCards",
            "cards": cards,
            "to": { "kind": "graveyard", "player": { "kind": "ownerOf", "object": self_ref() } },
        }),
        // Its owner's library (CR 400.3); the caller composes the shuffle of
        // that library (CR 701.24b), so the placement order is irrelevant.
        SourceZone::Library => json!({
            "kind": "moveCards",
            "cards": cards,
            "to": {
                "kind": "library",
                "player": { "kind": "ownerOf", "object": self_ref() },
                "position": "bottom",
            },
        }),
        SourceZone::Exile => json!({
            "kind": "moveCards",
            "cards": cards,
            "to": {
                "kind": "exile",
                "player": { "kind": "ownerOf", "object": self_ref() },
                "faceDown": false,
            },
        }),
        SourceZone::Battlefield => {
            let mut destination = json!({
                "kind": "battlefield",
                "player": match effect.controller {
                    SourceMoveController::Owner => {
                        json!({ "kind": "ownerOf", "object": self_ref() })
                    }
                    SourceMoveController::Preserve => controller(),
                },
                "tapped": effect.tapped,
            });
            if effect.face_change == SourceFaceChange::Transform {
                destination["faceChange"] = json!({ "kind": "transformFace" });
            }
            // CR 122.6: the counters are placed as the source enters. An
            // amount the shared numeric lowering cannot represent stays an
            // explicit unsupported operand (`null`), never zero.
            if let Some(counters) = &effect.entering_counters {
                destination["enterWithCounters"] = json!([{
                    "counter": counters.counter.as_str(),
                    "count": lower_numeric_expression(&counters.count).unwrap_or(Value::Null),
                }]);
            }
            json!({
                "kind": "moveCards",
                "cards": cards,
                "to": destination,
            })
        }
    };
    if let Some(origin) = origin {
        primitive["from"] = origin;
    }
    if let Some(binding) = bind {
        primitive["bind"] = json!(binding);
    }
    primitive
}
