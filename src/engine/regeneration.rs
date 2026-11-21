//! Regeneration shields (CR 701.15). A shield is a one-shot replacement for the
//! next destruction of one permanent this turn; it expires at cleanup with the
//! other `expiresAfterTurn` rule modifiers.
use super::*;

impl GameEngine {
    /// CR 701.15a: create one regeneration shield for a permanent still on the
    /// battlefield. Several shields may coexist; each is consumed separately.
    pub(super) fn install_regeneration_shield(&mut self, instance_id: &str) {
        if self.permanent_position(instance_id).is_some() {
            self.state.rule_modifiers.push(json!({
                "kind": "regenerationShield",
                "cardInstanceId": instance_id,
                "expiresAfterTurn": self.state.turn_number,
            }));
        }
    }

    /// CR 701.15b: consuming a shield replaces the destruction: the permanent
    /// is tapped, all damage is removed from it, and it is removed from combat.
    pub(super) fn consume_regeneration_shield(&mut self, instance_id: &str, reason: &str) {
        if let Some(index) = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some("regenerationShield")
                && modifier["cardInstanceId"].as_str() == Some(instance_id)
        }) {
            self.state.rule_modifiers.remove(index);
        }
        if let Some((player_index, card_index)) = self.permanent_position(instance_id) {
            let permanent = &mut self.state.players[player_index].battlefield[card_index];
            permanent.tapped = true;
            permanent.damage_marked = 0;
        }
        self.state
            .combat
            .attackers
            .retain(|attacker| attacker.attacker_id != instance_id);
        self.state.combat.blockers.retain(|blocker| {
            blocker.blocker_id != instance_id && blocker.attacker_id != instance_id
        });
        self.record_event(
            "permanentRegenerated",
            None,
            Some(instance_id.to_string()),
            json!({ "reason": reason }),
        );
    }
}
