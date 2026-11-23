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
        effect_domains::characteristic_object_supported(&effect["object"])
            && effect["subtypeClass"].as_str() == Some("creature")
            && subtype_set_mode_supported(effect)
            && matches!(
                value_kind(&effect["duration"]),
                Some("permanent" | "untilEndOfCurrentTurn")
            )
    })
}

/// `all` takes no subtype list; `replace` (CR 205.1b: the new creature types
/// replace the old ones) and `add` (`in addition to its other types`) require
/// a non-empty list of registered creature subtypes.
fn subtype_set_mode_supported(effect: &Value) -> bool {
    match effect["mode"].as_str() {
        Some("all") => effect.get("subtypes").is_none(),
        Some("replace" | "add") => effect["subtypes"].as_array().is_some_and(|subtypes| {
            !subtypes.is_empty()
                && subtypes.iter().all(|subtype| {
                    subtype
                        .as_str()
                        .is_some_and(super::object_predicate::is_creature_subtype)
                })
        }),
        _ => false,
    }
}

/// Effective membership of `subtype` decided by the object's subtype-set
/// markers, applied in timestamp (insertion) order. `None` means no marker
/// decides it and the printed subtypes and other sources apply. A `replace`
/// marker drops every earlier marker of its class when it is added, so the
/// fold below only ever sees the effects that are still in force.
pub(super) fn marker_subtype_membership(
    definition: &CardDefinition,
    subtype_class: &str,
    subtype: &str,
) -> Option<bool> {
    let mut membership = None;
    for rule in definition.printed_rules() {
        if value_kind(rule) != Some(MARKER_KIND)
            || rule["subtypeClass"].as_str() != Some(subtype_class)
        {
            continue;
        }
        let listed = rule["subtypes"].as_array().is_some_and(|subtypes| {
            subtypes
                .iter()
                .filter_map(Value::as_str)
                .any(|value| value.eq_ignore_ascii_case(subtype))
        });
        match rule["mode"].as_str() {
            Some("all") => membership = Some(true),
            Some("replace") => membership = Some(listed),
            Some("add") if listed => membership = Some(true),
            _ => {}
        }
    }
    membership
}

pub(super) fn definition_has_all_subtypes(
    definition: &CardDefinition,
    subtype_class: &str,
) -> bool {
    definition.printed_rules().iter().any(|rule| {
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
        decisions: &BTreeMap<String, Value>,
    ) {
        // CR 611.2c: the affected set is locked in when the effect begins.
        for instance_id in
            self.characteristic_object_ids(&effect["object"], stack_object, bindings, decisions)
        {
            self.modify_subtype_set_of(effect, stack_object, instance_id);
        }
    }

    fn modify_subtype_set_of(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        instance_id: String,
    ) {
        let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
            return;
        };

        let subtype_class = effect["subtypeClass"].as_str().unwrap_or_default();
        let mode = effect["mode"].as_str().unwrap_or_default();
        let temporary = value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn");
        let turn_number = self.state.turn_number;
        if mode != "all" {
            // CR 205.3d: an object cannot gain a creature subtype unless it
            // has a type that can carry one, read after earlier layer-4
            // effects of the same resolution.
            let permanent = &self.state.players[player_index].battlefield[card_index];
            let can_carry = self.permanent_is_creature(permanent)
                || has_card_type(&permanent.definition, "kindred")
                || has_card_type(&permanent.definition, "tribal");
            if !can_carry {
                return;
            }
        }
        let definition = &mut self.state.players[player_index].battlefield[card_index].definition;
        let rules = definition.printed_rules_mut();
        rules.retain(|rule| {
            !(value_kind(rule) == Some(MARKER_KIND)
                && rule["subtypeClass"].as_str() == Some(subtype_class)
                && (mode == "replace" || (mode == "all" && rule["mode"].as_str() == Some("all"))))
        });
        let mut marker = json!({
            "kind": MARKER_KIND,
            "subtypeClass": subtype_class,
            "mode": mode,
        });
        if mode != "all" {
            marker["subtypes"] = effect["subtypes"].clone();
        }
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
                "subtypes": effect["subtypes"],
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
