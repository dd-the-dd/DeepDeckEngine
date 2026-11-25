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
//! that creature attacked. The entry itself goes through the shared
//! attacking-entry writer (`entering_attacking`): it is never a declared
//! attacker (CR 508.4), so attack triggers do not fire for it.
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

/// A `sameDefenderAs` policy names the cost choice whose creature's
/// recorded defender the entering permanent attacks.
pub(super) fn same_defender_reference_supported(value: &Value) -> bool {
    value_kind(value) == Some("sameDefenderAs")
        && value_kind(&value["object"]) == Some("chosenObject")
        && value["object"]["id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
}

impl GameEngine {
    /// The defender the referenced creature was recorded attacking, read
    /// from the instance the activation captured (or a still-declared
    /// target of the same id). Its record survives its leaving combat.
    pub(super) fn same_defender_recorded(
        &self,
        policy: &Value,
        stack_object: &StackObject,
    ) -> Option<TargetRef> {
        let reference_id = policy["object"]["id"].as_str()?;
        let reference = match (
            stack_object
                .decisions
                .get(&reference_key(reference_id))
                .and_then(Value::as_str),
            stack_object.targets.get(reference_id),
        ) {
            (Some(captured), _) => captured.to_string(),
            (None, Some(TargetRef::Permanent { instance_id })) => instance_id.clone(),
            _ => return None,
        };
        self.state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == reference)
            .map(|assignment| assignment.defender.clone())
    }
}
