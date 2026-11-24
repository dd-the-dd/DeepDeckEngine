//! `remove OBJECT from combat` (CR 506.4): the permanent stops being an
//! attacking or blocking creature. A removed attacker deals and is dealt no
//! combat damage; a creature it was blocked by stays blocking nothing. An
//! attacker whose blocker is removed remains blocked (CR 509.1h).
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

/// The rules-state kind recording a removal for the rest of the turn.
const REMOVED_FROM_COMBAT: &str = "removedFromCombat";

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("removeFromCombat") => {
            target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
        }
        _ => false,
    })
}

impl GameEngine {
    pub(super) fn execute_combat_removal_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("removeFromCombat") {
            return Err(EngineError::new(format!(
                "unsupported effect reached execution: {}",
                value_kind(effect).unwrap_or("missingKind")
            )));
        }
        let mut instance_ids = self
            .resolve_runtime_object_ids(&effect["permanent"], stack_object, bindings, decisions)
            .unwrap_or_default();
        if instance_ids.is_empty()
            && let Some(TargetRef::Permanent { instance_id }) =
                self.resolve_target(&effect["permanent"], stack_object, bindings)
        {
            instance_ids.push(instance_id);
        }
        for instance_id in instance_ids {
            self.remove_permanent_from_combat(&instance_id);
        }
        Ok(true)
    }

    /// CR 506.4: the permanent is no longer an attacking or blocking
    /// creature. A permanent not in combat is unchanged.
    fn remove_permanent_from_combat(&mut self, instance_id: &str) {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let permanent = &mut self.state.players[player_index].battlefield[card_index];
        let attacking = permanent.flags.remove("attacking").unwrap_or(false);
        let blocking = permanent.flags.remove("blocking").unwrap_or(false);
        let declared = self
            .state
            .combat
            .attackers
            .iter()
            .any(|attacker| attacker.attacker_id == instance_id)
            || self
                .state
                .combat
                .blockers
                .iter()
                .any(|blocker| blocker.blocker_id == instance_id);
        if !(attacking || blocking || declared) {
            return;
        }
        self.state
            .combat
            .attackers
            .retain(|attacker| attacker.attacker_id != instance_id);
        self.state.combat.blockers.retain(|blocker| {
            blocker.blocker_id != instance_id && blocker.attacker_id != instance_id
        });
        self.state.rule_modifiers.push(json!({
            "kind": REMOVED_FROM_COMBAT,
            "cardInstanceId": instance_id,
            "turn": self.state.turn_number,
        }));
        let controller = self.state.players[player_index].battlefield[card_index]
            .controller
            .clone();
        self.record_event(
            "removedFromCombat",
            Some(controller),
            Some(instance_id.to_string()),
            json!({}),
        );
    }

    /// The combat damage step reads the declared attacks and blocks: the
    /// permanents removed from combat this turn leave them. A removed
    /// attacker leaves the attacks and its blocks; a removed blocker leaves
    /// the blocks of its attacker, which stays blocked.
    pub(in crate::engine) fn prune_removed_from_combat(
        &mut self,
        attacks: &mut BTreeMap<String, AttackAssignment>,
        blocks: &mut BTreeMap<String, Vec<String>>,
    ) {
        let turn = u64::from(self.state.turn_number);
        let removed = self
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some(REMOVED_FROM_COMBAT)
                    && modifier["turn"].as_u64() == Some(turn)
            })
            .filter_map(|modifier| modifier["cardInstanceId"].as_str())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        if removed.is_empty() {
            return;
        }
        attacks.retain(|attacker_id, _| !removed.contains(attacker_id));
        blocks.retain(|attacker_id, _| !removed.contains(attacker_id));
        for blockers in blocks.values_mut() {
            blockers.retain(|blocker_id| !removed.contains(blocker_id));
        }
    }
}
