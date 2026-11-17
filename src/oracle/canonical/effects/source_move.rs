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
        (SourceZone::Battlefield | SourceZone::Exile, _) => None,
    };
    let mut primitive = match effect.to {
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
                "tapped": false,
            });
            if effect.face_change == SourceFaceChange::Transform {
                destination["faceChange"] = json!({ "kind": "transformFace" });
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
