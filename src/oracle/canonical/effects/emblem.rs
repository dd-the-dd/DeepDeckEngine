use super::super::*;

mod additional_land_play;
mod no_maximum_hand_size;

pub(in crate::oracle::canonical) use additional_land_play::{
    claims_promoted_additional_land_play_family, parse_promoted_additional_land_play,
};
pub(in crate::oracle::canonical) use no_maximum_hand_size::{
    claims_promoted_no_maximum_hand_size_family, parse_promoted_no_maximum_hand_size,
};

fn lower_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) => {
            unreachable!("emblem grammar binds the ability controller")
        }
    }
}

fn lower_player_modifier(modifier: &PlayerModifierNode) -> Option<Value> {
    Some(match modifier {
        PlayerModifierNode::AdditionalLandPlay { player, count } => json!({
            "kind": "additionalLandPlay",
            "player": lower_player(player),
            "count": lower_numeric_expression(count)?,
        }),
        PlayerModifierNode::NoMaximumHandSize { player } => json!({
            "kind": "noMaximumHandSize",
            "player": lower_player(player),
        }),
    })
}

pub(in crate::oracle::canonical) fn lower_create_emblem_effect(
    effect: &CreateEmblemEffectNode,
) -> Value {
    json!({
        "kind": "createEmblem",
        "player": lower_player(&effect.player),
        "modifiers": effect
            .modifiers
            .iter()
            .map(|modifier| {
                lower_player_modifier(modifier)
                    .expect("emblem player modifiers are lowerable by construction")
            })
            .collect::<Vec<_>>(),
    })
}
