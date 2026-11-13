//! Runtime support for explicit subtype-set characteristic changes.
//!
//! These markers are characteristics, not abilities. In particular, an
//! `all` creature-subtype set remains effective while the permanent loses all
//! abilities. Markers live on the current object's mutable definition so the
//! existing battlefield-leave reset removes them with every zone change.

use super::*;

const MARKER_KIND: &str = "subtypeSetModification";

pub(super) fn effect_supported(effect: &Value) -> Option<bool> {
    (value_kind(effect) == Some("modifySubtypeSet")).then(|| {
        (matches!(
            value_kind(&effect["object"]),
            Some("self" | "abilitySource")
        ) || target_reference_supported(&effect["object"]))
            && effect["subtypeClass"].as_str() == Some("creature")
            && effect["mode"].as_str() == Some("all")
            && matches!(
                value_kind(&effect["duration"]),
                Some("permanent" | "untilEndOfCurrentTurn")
            )
    })
}

pub(super) fn definition_has_all_subtypes(
    definition: &CardDefinition,
    subtype_class: &str,
) -> bool {
    definition.rules.iter().any(|rule| {
        value_kind(rule) == Some(MARKER_KIND)
            && rule["subtypeClass"].as_str() == Some(subtype_class)
            && rule["mode"].as_str() == Some("all")
    })
}

impl GameEngine {
    pub(super) fn execute_modify_subtype_set(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) {
        let instance_id = match value_kind(&effect["object"]) {
            Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
            _ => self
                .resolve_target(&effect["object"], stack_object, bindings)
                .and_then(|target| match target {
                    TargetRef::Permanent { instance_id } => Some(instance_id),
                    _ => None,
                }),
        };
        let Some(instance_id) = instance_id else {
            return;
        };
        let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
            return;
        };

        let subtype_class = effect["subtypeClass"].as_str().unwrap_or_default();
        let mode = effect["mode"].as_str().unwrap_or_default();
        let temporary = value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn");
        let turn_number = self.state.turn_number;
        let rules = &mut self.state.players[player_index].battlefield[card_index]
            .definition
            .rules;
        rules.retain(|rule| {
            !(value_kind(rule) == Some(MARKER_KIND)
                && rule["subtypeClass"].as_str() == Some(subtype_class))
        });
        let mut marker = json!({
            "kind": MARKER_KIND,
            "subtypeClass": subtype_class,
            "mode": mode,
        });
        if temporary {
            marker["temporaryUntilTurn"] = Value::from(turn_number);
        }
        rules.push(marker);
        self.record_event(
            "subtypeSetModified",
            Some(stack_object.controller.clone()),
            Some(instance_id),
            json!({
                "subtypeClass": subtype_class,
                "mode": mode,
                "duration": effect["duration"],
            }),
        );
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "subtypes_tests.rs"]
mod tests;
