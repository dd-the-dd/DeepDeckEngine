use super::super::*;

/// Lowers one independently recognized source move. The ability envelope owns
/// cross-effect bindings, so the same primitive remains useful outside of
/// transform sequences.
pub(in crate::oracle::canonical) fn lower_source_move_effect(
    effect: &MoveSourceEffectNode,
    cards: Value,
    bind: Option<&str>,
) -> Value {
    let mut primitive = match effect.to {
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
    if let Some(binding) = bind {
        primitive["bind"] = json!(binding);
    }
    primitive
}
