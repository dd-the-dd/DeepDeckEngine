//! Runtime support for explicit subtype-set characteristic changes.
//!
//! These markers are characteristics, not abilities. In particular, an
//! `all` creature-subtype set remains effective while the permanent loses all
//! abilities. Markers live on the current object's mutable definition so the
//! existing battlefield-leave reset removes them with every zone change.

use super::*;

const MARKER_KIND: &str = "subtypeSetModification";

/// The basic land types (CR 205.3i, 305.6).
pub(super) const BASIC_LAND_TYPES: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

/// Each basic land type with the mana symbol of its intrinsic ability.
const BASIC_LAND_MANA: [(&str, &str); 5] = [
    ("Plains", "W"),
    ("Island", "U"),
    ("Swamp", "B"),
    ("Mountain", "R"),
    ("Forest", "G"),
];

/// The land types (CR 205.3i).
const LAND_TYPES: [&str; 18] = [
    "Cave",
    "Cloud",
    "Desert",
    "Forest",
    "Gate",
    "Island",
    "Lair",
    "Locus",
    "Mine",
    "Mountain",
    "Planet",
    "Plains",
    "Power-Plant",
    "Sphere",
    "Swamp",
    "Tower",
    "Town",
    "Urza's",
];

fn is_basic_land_type(subtype: &str) -> bool {
    BASIC_LAND_TYPES
        .iter()
        .any(|land_type| land_type.eq_ignore_ascii_case(subtype))
}

fn is_land_type(subtype: &str) -> bool {
    LAND_TYPES
        .iter()
        .any(|land_type| land_type.eq_ignore_ascii_case(subtype))
}

/// The subtype class a registered subtype belongs to: it decides which
/// subtype-set markers answer a membership query for that subtype.
pub(super) fn registered_subtype_class(subtype: &str) -> Option<&'static str> {
    if super::object_predicate::is_creature_subtype(subtype) {
        Some("creature")
    } else if is_land_type(subtype) {
        Some("land")
    } else {
        None
    }
}

pub(super) fn effect_supported(effect: &Value) -> Option<bool> {
    (value_kind(effect) == Some("modifySubtypeSet")).then(|| {
        effect_domains::characteristic_object_supported(&effect["object"])
            && match effect["subtypeClass"].as_str() {
                Some("creature") => subtype_set_mode_supported(effect),
                Some("land") => land_subtype_set_supported(effect),
                _ => false,
            }
            && matches!(
                value_kind(&effect["duration"]),
                Some("permanent" | "untilEndOfCurrentTurn")
            )
    })
}

/// Land subtypes (CR 205.3i): `replace` sets the object's basic land types
/// (CR 305.7: `becomes an Island`, `the basic land type of your choice`);
/// `add` adds land types (`an Island in addition to its other types`). A
/// chosen type reads a `chooseBasicLandType` of the same resolution.
fn land_subtype_set_supported(effect: &Value) -> bool {
    let listed = |accepted: fn(&str) -> bool| {
        effect["subtypes"].as_array().is_some_and(|subtypes| {
            !subtypes.is_empty()
                && subtypes
                    .iter()
                    .all(|subtype| subtype.as_str().is_some_and(accepted))
        })
    };
    let chosen = value_kind(&effect["subtypes"]) == Some("decisionResult")
        && effect["subtypes"]["decisionId"].is_string();
    match effect["mode"].as_str() {
        Some("replace") => listed(is_basic_land_type) || chosen,
        Some("add") => listed(is_land_type) || chosen,
        _ => false,
    }
}

/// Whether an effect set this land's subtype to basic land types: the land
/// then loses its old land types and the abilities of its rules text
/// (CR 305.7).
pub(super) fn land_rules_text_replaced(definition: &CardDefinition) -> bool {
    definition.printed_rules().iter().any(|rule| {
        value_kind(rule) == Some(MARKER_KIND)
            && rule["subtypeClass"].as_str() == Some("land")
            && rule["mode"].as_str() == Some("replace")
    })
}

/// The intrinsic mana abilities of the object's basic land types (CR
/// 305.6), after the land subtype-set markers recorded on it (CR 305.7):
/// one symbol list per ability.
pub(super) fn effective_basic_land_mana(definition: &CardDefinition) -> Vec<Vec<String>> {
    let mut mana = BASIC_LAND_MANA
        .iter()
        .filter(|(land_type, _)| {
            marker_subtype_membership(definition, "land", land_type)
                .unwrap_or_else(|| has_card_subtype(definition, land_type))
        })
        .map(|(_, symbol)| vec![(*symbol).to_string()])
        .collect::<Vec<_>>();
    // The colorless entry of the printed reading is not a land type
    // (CR 205.3i); a land whose rules text was replaced loses it.
    if !land_rules_text_replaced(definition) {
        mana.extend(
            basic_land_mana(definition)
                .into_iter()
                .filter(|symbols| symbols.iter().any(|symbol| symbol == "C")),
        );
    }
    mana
}

/// `all` takes no subtype list; `replace` (CR 205.1b: the new creature types
/// replace the old ones) and `add` (`in addition to its other types`) require
/// a non-empty list of registered creature subtypes.
fn subtype_set_mode_supported(effect: &Value) -> bool {
    match effect["mode"].as_str() {
        Some("all") => effect.get("subtypes").is_none(),
        Some("replace" | "add") => {
            effect["subtypes"].as_array().is_some_and(|subtypes| {
                !subtypes.is_empty()
                    && subtypes.iter().all(|subtype| {
                        subtype
                            .as_str()
                            .is_some_and(super::object_predicate::is_creature_subtype)
                    })
            })
                // `the creature type of your choice`: the subtype a
                // `chooseCreatureType` of the same resolution recorded.
                || (value_kind(&effect["subtypes"]) == Some("decisionResult")
                    && effect["subtypes"]["decisionId"].is_string())
        }
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
        // A chosen subtype reads the choice made earlier in this resolution.
        let chosen;
        let effect = if value_kind(&effect["subtypes"]) == Some("decisionResult") {
            let Some(subtypes) = effect["subtypes"]["decisionId"]
                .as_str()
                .and_then(|id| decisions.get(id))
                .filter(|subtypes| subtypes.as_array().is_some_and(|list| !list.is_empty()))
            else {
                // No subtype was chosen: nothing changes.
                return;
            };
            // A land-type set reads a chosen land type only (CR 205.3i).
            if effect["subtypeClass"].as_str() == Some("land")
                && !subtypes.as_array().is_some_and(|list| {
                    list.iter()
                        .all(|subtype| subtype.as_str().is_some_and(is_land_type))
                })
            {
                return;
            }
            let mut resolved = effect.clone();
            resolved["subtypes"] = subtypes.clone();
            chosen = resolved;
            &chosen
        } else {
            effect
        };
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
            // CR 205.3d: an object cannot gain a subtype unless it has a
            // type that can carry one, read after earlier layer-4 effects of
            // the same resolution: a creature subtype needs a creature (or
            // kindred), a land type a land.
            let permanent = &self.state.players[player_index].battlefield[card_index];
            let can_carry = if subtype_class == "land" {
                self.permanent_has_card_type(permanent, "Land")
            } else {
                self.permanent_is_creature(permanent)
                    || has_card_type(&permanent.definition, "kindred")
                    || has_card_type(&permanent.definition, "tribal")
            };
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "subtypes_land_tests.rs"]
mod land_tests;
