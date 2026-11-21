//! Resolved characteristic-setting effects (CR 611.2c, 613).
//!
//! Each primitive changes one characteristic of a set of permanents that is
//! locked in when the effect begins. The effect is recorded as a marker on the
//! affected object itself, so a zone change ends it (CR 400.7) and insertion
//! order is its timestamp among the effects recorded on that object (CR 613.7):
//!
//! - `setCardTypes` (layer 4, CR 205.1a): read by the effective card-type query;
//! - `setColors` (layer 5, CR 105.3): read by both effective color queries;
//! - `setBasePowerToughness` (layer 7b, CR 613.4b): read before counters and
//!   modifications (layers 7c-d).
//!
//! The markers are characteristics, not abilities: an object that loses all
//! abilities keeps them. `untilEndOfCurrentTurn` markers carry the existing
//! `temporaryUntilTurn` cleanup contract (CR 514.2); `permanent` ones last
//! while the object stays on the battlefield.
use super::super::*;

pub(in crate::engine) const CARD_TYPE_SET_MARKER: &str = "cardTypeSetModification";
pub(in crate::engine) const COLOR_SET_MARKER: &str = "colorSetModification";
pub(in crate::engine) const BASE_POWER_TOUGHNESS_MARKER: &str = "basePowerToughnessModification";

const PERMANENT_CARD_TYPES: [&str; 6] = [
    "Artifact",
    "Battle",
    "Creature",
    "Enchantment",
    "Land",
    "Planeswalker",
];

/// The object operand shared by the characteristic primitives: the source,
/// one declared target, or a resolved object expression (target collections
/// and battlefield collections).
pub(in crate::engine) fn characteristic_object_supported(object: &Value) -> bool {
    matches!(value_kind(object), Some("self" | "abilitySource"))
        || target_reference_supported(object)
        || object_expression_supported(object)
}

fn duration_supported(duration: &Value) -> bool {
    matches!(
        value_kind(duration),
        Some("permanent" | "untilEndOfCurrentTurn")
    )
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(
        characteristic_object_supported(&effect["object"])
            && duration_supported(&effect["duration"])
            && match value_kind(effect) {
                Some("setCardTypes") => effect["cardTypes"].as_array().is_some_and(|types| {
                    !types.is_empty()
                        && types.iter().all(|card_type| {
                            card_type
                                .as_str()
                                .is_some_and(|value| PERMANENT_CARD_TYPES.contains(&value))
                        })
                }),
                Some("setColors") => effect["colors"].as_array().is_some_and(|colors| {
                    colors
                        .iter()
                        .all(|color| matches!(color.as_str(), Some("W" | "U" | "B" | "R" | "G")))
                }),
                Some("setBasePowerToughness") => {
                    numeric_expression_supported(&effect["power"])
                        && numeric_expression_supported(&effect["toughness"])
                }
                _ => false,
            },
    )
}

fn latest_marker<'a>(definition: &'a CardDefinition, kind: &str) -> Option<&'a Value> {
    definition
        .rules
        .iter()
        .rev()
        .find(|rule| value_kind(rule) == Some(kind))
}

fn marker_strings(marker: &Value, field: &str) -> Vec<String> {
    marker[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

/// The latest card-type set recorded on this object, if any (CR 205.1a).
pub(in crate::engine) fn marker_card_types(definition: &CardDefinition) -> Option<Vec<String>> {
    latest_marker(definition, CARD_TYPE_SET_MARKER)
        .map(|marker| marker_strings(marker, "cardTypes"))
}

/// The latest color set recorded on this object, if any (CR 105.3).
pub(in crate::engine) fn marker_colors(definition: &CardDefinition) -> Option<Vec<String>> {
    latest_marker(definition, COLOR_SET_MARKER).map(|marker| marker_strings(marker, "colors"))
}

/// The latest base power/toughness recorded on this object (CR 613.4b).
pub(in crate::engine) fn marker_base_power_toughness(
    definition: &CardDefinition,
) -> Option<(i32, i32)> {
    latest_marker(definition, BASE_POWER_TOUGHNESS_MARKER).and_then(|marker| {
        Some((
            i32::try_from(marker["power"].as_i64()?).ok()?,
            i32::try_from(marker["toughness"].as_i64()?).ok()?,
        ))
    })
}

impl GameEngine {
    /// The permanents an effect affects, determined once when it begins
    /// (CR 611.2c). A missing or departed object is a rules no-op.
    pub(in crate::engine) fn characteristic_object_ids(
        &self,
        object: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        let ids = match value_kind(object) {
            Some("self" | "abilitySource") => vec![stack_object.card.instance_id.clone()],
            _ if target_reference_supported(object) => self
                .resolve_target(object, stack_object, bindings)
                .and_then(|target| match target {
                    TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                    _ => None,
                })
                .unwrap_or_default(),
            _ => self
                .resolve_runtime_object_ids(object, stack_object, bindings, decisions)
                .unwrap_or_default(),
        };
        ids.into_iter()
            .filter(|instance_id| self.permanent_position(instance_id).is_some())
            .collect()
    }

    pub(super) fn execute_characteristic_layers_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        let (marker_kind, mut marker, event, payload) = match value_kind(effect) {
            Some("setCardTypes") => (
                CARD_TYPE_SET_MARKER,
                json!({ "cardTypes": effect["cardTypes"] }),
                "cardTypesSet",
                json!({ "cardTypes": effect["cardTypes"], "duration": effect["duration"] }),
            ),
            Some("setColors") => (
                COLOR_SET_MARKER,
                json!({ "colors": effect["colors"] }),
                "colorsSet",
                json!({ "colors": effect["colors"], "duration": effect["duration"] }),
            ),
            Some("setBasePowerToughness") => {
                let power = self.checked_runtime_numeric_expression(
                    &effect["power"],
                    stack_object,
                    bindings,
                    decisions,
                    "setBasePowerToughness.power",
                )?;
                let toughness = self.checked_runtime_numeric_expression(
                    &effect["toughness"],
                    stack_object,
                    bindings,
                    decisions,
                    "setBasePowerToughness.toughness",
                )?;
                (
                    BASE_POWER_TOUGHNESS_MARKER,
                    json!({ "power": power, "toughness": toughness }),
                    "basePowerToughnessChanged",
                    json!({
                        "power": power,
                        "toughness": toughness,
                        "duration": effect["duration"],
                    }),
                )
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        };
        marker["kind"] = Value::from(marker_kind);
        if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn") {
            marker["temporaryUntilTurn"] = Value::from(self.state.turn_number);
        }
        // CR 611.2c: the affected set is locked in when the effect begins.
        for instance_id in
            self.characteristic_object_ids(&effect["object"], stack_object, bindings, decisions)
        {
            let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            self.state.players[player_index].battlefield[card_index]
                .definition
                .rules
                .push(marker.clone());
            self.record_event(
                event,
                Some(stack_object.controller.clone()),
                Some(instance_id),
                payload.clone(),
            );
        }
        Ok(true)
    }
}
