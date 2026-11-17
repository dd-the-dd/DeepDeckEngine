//! Canonical populations shared by action parents after their own scope checks.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_player_set_reference(
    players: PlayerSetReferenceNode,
) -> Value {
    match players {
        PlayerSetReferenceNode::EachPlayer => json!({"kind":"eachPlayer"}),
        PlayerSetReferenceNode::OpponentsOfAbilityController => {
            json!({"kind":"opponentsOf", "player":controller()})
        }
    }
}
