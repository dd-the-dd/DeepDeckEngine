//! Atomic programs for runtime-generated spell-copy triggers.
//! Keyword identities choose the copy count before this adapter; the executor
//! receives only the existing copyStackItem primitive and the triggering stack snapshot.

use super::{CardInstance, GameEngine};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn spell_copy_trigger(copy_count: usize, may_choose_new_targets: bool) -> Value {
    let effect = json!({
        "kind": "copyStackItem",
        "object": { "kind": "triggeringStackObject" },
        "controller": { "kind": "abilityController" },
        "mayChooseNewTargets": may_choose_new_targets,
    });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "effects": vec![effect; copy_count],
    })
}

impl GameEngine {
    pub(super) fn put_spell_copy_trigger_on_stack(
        &mut self,
        source: &CardInstance,
        original_stack_id: &str,
        copy_count: usize,
        may_choose_new_targets: bool,
        mut decisions: BTreeMap<String, Value>,
        origin: super::trigger_multiplicity::TriggerOrigin,
    ) {
        if self.check_quantity_failure().is_err() {
            return;
        }
        decisions.insert(
            "triggeringStackId".to_string(),
            Value::String(original_stack_id.to_string()),
        );
        if let Some(original) = self
            .state
            .stack
            .iter()
            .find(|original| original.id == original_stack_id)
        {
            decisions.insert("triggeringStackSnapshot".to_string(), json!(original));
        }
        self.put_triggered_ability_on_stack_with_origin(
            source,
            spell_copy_trigger(copy_count, may_choose_new_targets),
            decisions,
            origin,
        );
        if self.check_quantity_failure().is_err() {
            return;
        }
    }
    /// Refresh only existing typed triggering-object snapshots when that stack
    /// object leaves. This captures retargeting and later modifications rather
    /// than treating the cast-time value as immutable last-known information.
    pub(super) fn remember_departing_stack_object(&mut self, original: &super::StackObject) {
        let mut snapshot = None;
        for trigger in &mut self.state.stack {
            if trigger
                .decisions
                .get("triggeringStackId")
                .and_then(Value::as_str)
                == Some(original.id.as_str())
                && trigger.decisions.contains_key("triggeringStackSnapshot")
            {
                let captured = snapshot.get_or_insert_with(|| json!(original));
                trigger
                    .decisions
                    .insert("triggeringStackSnapshot".to_string(), captured.clone());
            }
        }
    }

    pub(super) fn take_stack_object_with_lki(&mut self, index: usize) -> super::StackObject {
        let original = self.state.stack.remove(index);
        self.remember_departing_stack_object(&original);
        original
    }

    pub(super) fn pop_stack_object_with_lki(&mut self) -> Option<super::StackObject> {
        let original = self.state.stack.pop()?;
        self.remember_departing_stack_object(&original);
        Some(original)
    }

    pub(super) fn retain_stack_objects_with_lki<F>(&mut self, mut keep: F)
    where
        F: FnMut(&super::StackObject) -> bool,
    {
        let mut departing = Vec::new();
        self.state.stack.retain(|original| {
            if keep(original) {
                true
            } else {
                departing.push(original.clone());
                false
            }
        });
        for original in departing {
            self.remember_departing_stack_object(&original);
        }
    }

    /// A trigger can outlive the original spell. Only the typed triggering-object
    /// reference may consume its captured last-known stack value; an ordinary
    /// targeted copy still requires the targeted object to exist.
    pub(super) fn copy_stack_item_from_effect(
        &mut self,
        original_stack_id: &str,
        effect: &Value,
        trigger: &super::StackObject,
    ) -> Option<String> {
        if self.check_quantity_failure().is_err() {
            return None;
        }
        if let Some(copy) = self.copy_stack_object(original_stack_id) {
            return Some(copy);
        }
        if super::value_kind(&effect["object"]) != Some("triggeringStackObject")
            || trigger
                .decisions
                .get("triggeringStackId")
                .and_then(Value::as_str)
                != Some(original_stack_id)
        {
            return None;
        }
        let snapshot = serde_json::from_value::<super::StackObject>(
            trigger.decisions.get("triggeringStackSnapshot")?.clone(),
        )
        .ok()?;
        if snapshot.id != original_stack_id {
            return None;
        }
        Some(self.copy_stack_object_value(&snapshot, original_stack_id))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "copy_program_tests.rs"]
mod tests;
