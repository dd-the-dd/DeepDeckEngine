//! Current or captured identity of an object, distinct from ability control.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    CardInstance, GameEngine, RuntimeBinding, StackObject, TargetRef, source_lki,
    target_reference_supported, value_kind,
};

pub(super) struct ObjectContext<'a> {
    pub(super) card: &'a CardInstance,
    pub(super) controller_id: &'a str,
    pub(super) on_battlefield: bool,
    pub(super) last_known: bool,
}

pub(super) fn object_context_reference_supported(reference: &Value) -> bool {
    matches!(value_kind(reference), Some("self" | "abilitySource"))
        || target_reference_supported(reference)
}

impl GameEngine {
    fn source_object_context<'a>(&'a self, stack_object: &'a StackObject) -> ObjectContext<'a> {
        let live = self
            .permanent_position(&stack_object.card.instance_id)
            .map(|(player, index)| &self.state.players[player].battlefield[index]);
        let source = source_lki::source_object_input(stack_object, live);
        ObjectContext {
            card: source.card,
            controller_id: if stack_object.ability_kind.is_none() && !source.on_battlefield {
                &stack_object.controller
            } else {
                &source.card.controller
            },
            on_battlefield: source.on_battlefield,
            last_known: source.last_known,
        }
    }

    /// Only the source has an authenticated pending LKI snapshot here. A
    /// different absent object must not borrow the source's controller or
    /// invent an incarnation; its caller needs a separately captured context.
    pub(super) fn object_context_for_reference<'a>(
        &'a self,
        reference: &Value,
        stack_object: &'a StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<ObjectContext<'a>> {
        if matches!(value_kind(reference), Some("self" | "abilitySource")) {
            return Some(self.source_object_context(stack_object));
        }
        match self.resolve_target(reference, stack_object, bindings)? {
            TargetRef::Permanent { instance_id } => {
                if instance_id == stack_object.card.instance_id {
                    return Some(self.source_object_context(stack_object));
                }
                let (player, index) = self.permanent_position(&instance_id)?;
                let card = &self.state.players[player].battlefield[index];
                Some(ObjectContext {
                    card,
                    controller_id: &card.controller,
                    on_battlefield: true,
                    last_known: false,
                })
            }
            TargetRef::StackObject { stack_id } => {
                let object = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)?;
                Some(ObjectContext {
                    card: &object.card,
                    controller_id: &object.controller,
                    on_battlefield: false,
                    last_known: false,
                })
            }
            TargetRef::Card { .. } | TargetRef::Player { .. } => None,
        }
    }

    pub(super) fn resolved_object_controller(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<String> {
        self.object_context_for_reference(reference, stack_object, bindings)
            .map(|context| context.controller_id.to_string())
    }

    pub(super) fn resolved_object_is_on_battlefield(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> bool {
        self.object_context_for_reference(reference, stack_object, bindings)
            .is_some_and(|context| context.on_battlefield)
    }

    pub(super) fn ability_signal_object_descriptor(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<(String, Value)> {
        let context = self.object_context_for_reference(reference, stack_object, bindings)?;
        let id = context.card.instance_id.clone();
        let descriptor = json!({
            "cardInstanceId": id,
            "definitionId": context.card.definition.id,
            "controllerId": context.controller_id,
            "zoneIdentity": Self::tracked_zone_identity(context.card),
            "lastKnown": context.last_known,
        });
        Some((id, descriptor))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_context_tests.rs"]
mod tests;
