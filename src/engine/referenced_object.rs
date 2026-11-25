//! `referencedObject`: the one-object collection a typed single-object
//! reference names, for consumers that act on collections (card moves,
//! exclusions). It adapts the shared reference resolver; it never chooses,
//! declares or mints an object.
//!
//! The reference keeps its own identity rule (CR 400.7): a declared target
//! that became illegal, an event object or captured delayed operand that is
//! now a new object, or a bound object that is gone names nothing, and the
//! collection is empty. The ability's source is the source card wherever it
//! is, unless it left the battlefield after the ability was created, in
//! which case it is a new object the ability cannot find.
use super::*;

/// The single-object references the adapter resolves.
pub(super) fn referenced_object_supported(expression: &Value) -> bool {
    let object = &expression["object"];
    expression
        .as_object()
        .is_some_and(|fields| fields.len() == 2)
        && value_kind(expression) == Some("referencedObject")
        && (matches!(value_kind(object), Some("self" | "abilitySource"))
            || target_reference_supported(object)
            || entry_observation::observed_object_reference_supported(object)
            || (value_kind(object) == Some("decisionResult") && object["decisionId"].is_string()))
}

impl GameEngine {
    /// The objects a `referencedObject` names: none or one.
    pub(super) fn referenced_object_ids(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Vec<String>> {
        if !referenced_object_supported(expression) {
            return None;
        }
        let object = &expression["object"];
        match value_kind(object) {
            Some("self" | "abilitySource") => Some(self.referenced_source_card(stack_object)),
            // An action-result reader resolved to the choice that fixed the
            // object (CR 608.2c).
            Some("decisionResult") => {
                let id = object["decisionId"].as_str()?;
                Some(
                    decisions
                        .get(id)?
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .take(1)
                        .map(str::to_string)
                        .collect(),
                )
            }
            _ => Some(match self.resolve_target(object, stack_object, bindings) {
                Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                    vec![instance_id]
                }
                Some(TargetRef::Player { .. } | TargetRef::StackObject { .. }) | None => Vec::new(),
            }),
        }
    }

    /// Moves a battlefield permanent a card move names to its owner's hand
    /// or library through the permanent's own departure lifecycle (CR 400.3,
    /// 400.7): the generic card mover handles only cards outside the
    /// battlefield. `false` when the object is no permanent or the
    /// destination has no such lifecycle here, so the caller's mover keeps
    /// its own contract (and its own explicit failure).
    pub(super) fn move_referenced_permanent(
        &mut self,
        instance_id: &str,
        destination: &Value,
    ) -> Result<bool, EngineError> {
        if self.permanent_position(instance_id).is_none() {
            return Ok(false);
        }
        match value_kind(destination) {
            Some("hand") => {
                self.return_permanent_to_hand(instance_id)?;
                Ok(true)
            }
            Some("library") if destination.get("cardsAbove").is_none() => {
                let position = match destination["position"].as_str() {
                    Some(position @ ("top" | "bottom")) => position,
                    _ => return Ok(false),
                };
                self.put_permanent_on_owner_library(instance_id, position)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// The source card as the object the ability refers to: the battlefield
    /// incarnation the ability was created for, or the card in another zone
    /// when the ability was created there. A source that left the battlefield
    /// after the ability was created, or a battlefield object of another
    /// incarnation, is a new object (CR 400.7).
    fn referenced_source_card(&self, stack_object: &StackObject) -> Vec<String> {
        if source_lki::source_has_departed(stack_object) {
            return Vec::new();
        }
        let instance_id = &stack_object.card.instance_id;
        if let Some((player, index)) = self.permanent_position(instance_id) {
            let live = &self.state.players[player].battlefield[index];
            let expected = Self::tracked_zone_identity(&stack_object.card);
            if expected.is_some_and(|identity| !identity.is_empty())
                && Self::tracked_zone_identity(live) != expected
            {
                return Vec::new();
            }
        }
        vec![instance_id.clone()]
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "typed_reference_consumers_tests.rs"]
mod tests;
