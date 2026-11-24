//! A permanent put onto the battlefield attacking what another creature was
//! attacking (CR 506.3a, 508.4): ninjutsu's "enters attacking the same
//! player, planeswalker, or battle as the creature that was returned to its
//! owner's hand" (CR 702.49c).
//!
//! ```json
//! {"kind": "sameDefenderAs",
//!  "object": {"kind": "chosenObject", "id": "activationCostReturn2"}}
//! ```
//!
//! The referenced creature is the one the ability's cost chose. A cost
//! choice is not a target of the ability on the stack, so the activation
//! captures the chosen instance in the stack object's decisions
//! (`cost_attack_references`). Its attack record survives its leaving the
//! battlefield, so the entering permanent reads the defender recorded when
//! that creature attacked. It is never a declared attacker (CR 508.4):
//! attack triggers do not fire for it.
use super::*;

/// The decision holding the instance a `sameDefenderAs` reference chose.
fn reference_key(choice_id: &str) -> String {
    format!("sameDefenderAs:{choice_id}")
}

/// The cost choices the rule's `sameDefenderAs` entries name, captured when
/// the ability is activated: one decision per referenced choice.
pub(super) fn cost_attack_references(
    rule: &Value,
    targets: &BTreeMap<String, TargetRef>,
) -> BTreeMap<String, Value> {
    rule["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|effect| value_kind(&effect["attacking"]) == Some("sameDefenderAs"))
        .filter_map(|effect| effect["attacking"]["object"]["id"].as_str())
        .filter_map(|choice_id| match targets.get(choice_id) {
            Some(TargetRef::Permanent { instance_id }) => {
                Some((reference_key(choice_id), Value::String(instance_id.clone())))
            }
            _ => None,
        })
        .collect()
}

/// The activation's decisions with the captured cost attack references.
pub(super) fn with_cost_attack_references(
    mut decisions: BTreeMap<String, Value>,
    rule: &Value,
    targets: &BTreeMap<String, TargetRef>,
) -> BTreeMap<String, Value> {
    decisions.extend(cost_attack_references(rule, targets));
    decisions
}

/// The `attacking` policy of an entry onto the battlefield.
pub(super) fn attacking_entry_supported(value: &Value) -> bool {
    value_kind(value) == Some("sameDefenderAs")
        && value_kind(&value["object"]) == Some("chosenObject")
        && value["object"]["id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
}

impl GameEngine {
    /// Makes the permanent that just entered attack the referenced
    /// creature's recorded defender. Outside combat, under the control of a
    /// player other than the active player, or when that player,
    /// planeswalker or battle can no longer be attacked, the permanent enters
    /// but is not attacking (CR 506.3, 508.4a).
    pub(super) fn put_entered_permanent_attacking(
        &mut self,
        instance_id: &str,
        attacking: &Value,
        stack_object: &StackObject,
    ) {
        let Some(reference_id) = attacking["object"]["id"].as_str() else {
            return;
        };
        let reference = match (
            stack_object
                .decisions
                .get(&reference_key(reference_id))
                .and_then(Value::as_str),
            stack_object.targets.get(reference_id),
        ) {
            (Some(captured), _) => captured.to_string(),
            (None, Some(TargetRef::Permanent { instance_id })) => instance_id.clone(),
            _ => return,
        };
        let Some(assignment) = self
            .state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == reference)
            .cloned()
        else {
            return;
        };
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let controller = self.state.players[player_index].battlefield[card_index]
            .controller
            .clone();
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        let in_combat = matches!(
            self.state.step,
            GameStep::DeclareAttackers | GameStep::DeclareBlockers | GameStep::CombatDamage
        );
        if !in_combat
            || controller != active_player_id
            || self
                .attack_defending_player_id(&active_player_id, &assignment.defender)
                .is_none()
        {
            return;
        }
        self.state.players[player_index].battlefield[card_index]
            .flags
            .insert("attacking".to_string(), true);
        self.state.combat.attackers.push(DeclaredAttacker {
            attacker_id: instance_id.to_string(),
            defender: assignment.defender,
            defending_player_id: assignment.defending_player_id,
        });
    }
}
