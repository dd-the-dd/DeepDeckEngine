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
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
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
            Some("setColors") => {
                effect["colors"].as_array().is_some_and(|colors| {
                    colors
                        .iter()
                        .all(|color| matches!(color.as_str(), Some("W" | "U" | "B" | "R" | "G")))
                }) || (value_kind(&effect["colors"]) == Some("decisionResult")
                    && effect["colors"]["decisionId"].is_string())
            }
            Some("setBasePowerToughness") => {
                numeric_expression_supported(&effect["power"])
                    && numeric_expression_supported(&effect["toughness"])
            }
            _ => false,
        }
}

fn latest_marker<'a>(definition: &'a CardDefinition, kind: &str) -> Option<&'a Value> {
    definition
        .printed_rules()
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

/// The colors of a `setColors`: a stated list, or the color symbols the
/// controller chose for this effect (`the color or colors of your choice`).
/// A missing or malformed choice is an error, never an empty color set.
fn chosen_or_stated_colors(
    colors: &Value,
    decisions: &BTreeMap<String, Value>,
) -> Result<Value, EngineError> {
    if value_kind(colors) != Some("decisionResult") {
        return Ok(colors.clone());
    }
    let chosen = colors["decisionId"]
        .as_str()
        .and_then(|id| decisions.get(id))
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::new("setColors reads a missing color choice"))?;
    if !chosen
        .iter()
        .all(|color| matches!(color.as_str(), Some("W" | "U" | "B" | "R" | "G")))
    {
        return Err(EngineError::new(
            "setColors reads a choice that is not color symbols",
        ));
    }
    Ok(Value::Array(chosen.clone()))
}

impl GameEngine {
    /// A characteristic-defining modifier (CR 604.3) is applied in layer 7a
    /// by [`Self::characteristic_defining_power_toughness`], never as a
    /// layer-7b setting effect of a battlefield static ability.
    pub(in crate::engine) fn is_layer_7a_modifier(modifier: &Value) -> bool {
        modifier["layer"].as_str() == Some("7a")
    }

    /// A characteristic marker a resolved effect recorded on an object: not
    /// a copiable value (CR 707.2), so a copy drops it.
    pub(in crate::engine) fn is_resolved_characteristic_marker(rule: &Value) -> bool {
        rule.get("temporaryUntilTurn").is_some()
            || matches!(
                value_kind(rule),
                Some(
                    CARD_TYPE_SET_MARKER
                        | COLOR_SET_MARKER
                        | BASE_POWER_TOUGHNESS_MARKER
                        | "subtypeSetModification"
                )
            )
    }

    /// A color set that is part of a copy's copiable values (CR 707.9b):
    /// the same characteristic the effective color queries read, without a
    /// duration, so it lasts as long as the copy.
    pub(in crate::engine) fn copiable_color_set(colors: &Value) -> Value {
        json!({ "kind": COLOR_SET_MARKER, "colors": colors, "copiable": true })
    }

    /// The layer-7a modifiers of `card`'s own characteristic-defining
    /// abilities. On the battlefield they come from the active static sites,
    /// so an object that lost its abilities (layer 6) no longer defines its
    /// power and toughness; in any other zone a CDA functions from the card's
    /// printed rules (CR 604.3, 113.6).
    fn characteristic_defining_modifiers(&self, card: &CardInstance) -> Vec<Value> {
        let defines_self = |modifier: &Value| {
            Self::is_layer_7a_modifier(modifier) && value_kind(&modifier["objects"]) == Some("self")
        };
        if self.permanent_position(&card.instance_id).is_some() {
            return self
                .battlefield_static_modifier_sites()
                .into_iter()
                .filter(|site| {
                    site.source.instance_id == card.instance_id
                        && self.static_modifier_site_is_active(site)
                        && defines_self(&site.modifier)
                })
                .map(|site| site.modifier)
                .collect();
        }
        card.definition
            .printed_rules()
            .iter()
            .filter(|rule| value_kind(rule) == Some("staticAbility"))
            .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
            .filter(|modifier| defines_self(modifier))
            .cloned()
            .collect()
    }

    /// Layer 7a (CR 613.4a): the base power and toughness defined by the
    /// object's own characteristic-defining abilities, each amount evaluated
    /// with the object as its source. A stat no CDA defines keeps its printed
    /// value; `None` when no CDA applies.
    pub(in crate::engine) fn characteristic_defining_power_toughness(
        &self,
        card: &CardInstance,
    ) -> Option<(i32, i32)> {
        let mut power = None;
        let mut toughness = None;
        for modifier in self.characteristic_defining_modifiers(card) {
            match value_kind(&modifier) {
                Some("setBasePowerToughness") => {
                    power = Some(self.static_numeric_expression(card, &modifier["power"]));
                    toughness = Some(self.static_numeric_expression(card, &modifier["toughness"]));
                }
                Some("setPower") => {
                    power = Some(self.static_numeric_expression(card, &modifier["power"]));
                }
                Some("setToughness") => {
                    toughness = Some(self.static_numeric_expression(card, &modifier["toughness"]));
                }
                _ => {}
            }
        }
        if power.is_none() && toughness.is_none() {
            return None;
        }
        Some((
            power.unwrap_or_else(|| parse_stat(&card.definition.power)),
            toughness.unwrap_or_else(|| parse_stat(&card.definition.toughness)),
        ))
    }

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
        self.execute_characteristic_layers_domain_known(effect, stack_object, bindings, decisions)
            .map(|()| true)
    }

    fn execute_characteristic_layers_domain_known(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let (marker_kind, mut marker, event, payload) = match value_kind(effect) {
            Some("setCardTypes") => (
                CARD_TYPE_SET_MARKER,
                json!({ "cardTypes": effect["cardTypes"] }),
                "cardTypesSet",
                json!({ "cardTypes": effect["cardTypes"], "duration": effect["duration"] }),
            ),
            Some("setColors") => {
                let colors = chosen_or_stated_colors(&effect["colors"], decisions)?;
                (
                    COLOR_SET_MARKER,
                    json!({ "colors": colors }),
                    "colorsSet",
                    json!({ "colors": colors, "duration": effect["duration"] }),
                )
            }
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
                .printed_rules_mut()
                .push(marker.clone());
            self.record_event(
                event,
                Some(stack_object.controller.clone()),
                Some(instance_id),
                payload.clone(),
            );
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "characteristic_layers_tests.rs"]
mod tests;
