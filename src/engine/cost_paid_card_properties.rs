//! The numeric characteristics of the card a cost of an ability paid with
//! (`the discarded card's mana value`, `the exiled card's power`, `the
//! revealed card's mana value`, CR 601.2h, 602.2b), recorded when the cost
//! is paid and read as that card's last known information (CR 608.2h).
//!
//! The producing cost carries the stack value it records (`bindPowerAs`,
//! `bindToughnessAs`, `bindManaValueAs`), installed by card composition
//! (`contextual_binding.rs`) only on the one chosen-card cost the ability
//! reads. A card is read by its printed characteristics: it is not on the
//! battlefield, so no continuous effect of a permanent applies to it. A
//! power or toughness that is not a printed number (a characteristic-
//! defining `*`) records a deferred read of that card instead: its
//! characteristic-defining ability functions in every zone (CR 604.3), so
//! the value is the card's current one where it now is when the effect
//! reads it (CR 608.2h). A deferred read whose card left that zone, or whose
//! ability the engine can't evaluate, stays unresolved; its consumers refuse
//! it instead of substituting zero ([`unresolved_decision_read`]).
use super::*;

/// The deferred characteristic read recorded for a `*` power or toughness.
const DEFERRED_CARD_CHARACTERISTIC: &str = "costPaidCardCharacteristic";

/// The first `decisionResult` read of `expression` that `decisions` does not
/// resolve to a number, if any: a consumer that would otherwise default a
/// missing amount refuses the effect explicitly instead (AGENTS.md: never
/// substitute zero for a missing producer).
pub(super) fn unresolved_decision_read<'a>(
    engine: &GameEngine,
    expression: &'a Value,
    decisions: &BTreeMap<String, Value>,
) -> Option<&'a str> {
    match expression {
        Value::Object(fields) => {
            if value_kind(expression) == Some("decisionResult") {
                let id = expression["decisionId"].as_str()?;
                let resolved = decisions.get(id).is_some_and(|value| {
                    value.as_i64().is_some() || engine.deferred_card_characteristic(value).is_some()
                });
                return (!resolved).then_some(id);
            }
            fields
                .values()
                .find_map(|child| unresolved_decision_read(engine, child, decisions))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| unresolved_decision_read(engine, child, decisions)),
        _ => None,
    }
}

/// The chosen card a card cost names: `discardCard` and `revealCard` name it
/// as `card`, `exileObject` as `object`. A cost paid with the ability's own
/// source names no chosen card and is not recorded here.
fn chosen_card_id(cost: &Value) -> Option<&str> {
    let operand = match value_kind(cost) {
        Some("discardCard" | "revealCard") => &cost["card"],
        Some("exileObject") => &cost["object"],
        _ => return None,
    };
    if !matches!(value_kind(operand), Some("chosenObject" | "chosenTarget")) {
        return None;
    }
    operand["id"].as_str()
}

/// A printed power or toughness that is a number.
fn printed_number(value: Option<&str>) -> Option<i32> {
    value?.trim().parse().ok()
}

impl GameEngine {
    /// Records the requested characteristics of every chosen card a cost of
    /// `costs` pays with, under the stack values the cost names. Called with
    /// the chosen cost objects of the activation or the cast (`targets`)
    /// while they are still the cards that were chosen.
    pub(super) fn record_cost_paid_card_properties(
        &self,
        costs: &Value,
        targets: &BTreeMap<String, TargetRef>,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        for cost in costs.as_array().into_iter().flatten() {
            let Some(card) = chosen_card_id(cost)
                .and_then(|id| targets.get(id))
                .and_then(|target| match target {
                    TargetRef::Card { instance_id } => self.card_outside_battlefield(instance_id),
                    _ => None,
                })
            else {
                continue;
            };
            let definition = &card.definition;
            let deferred = |characteristic: &str| {
                json!({
                    "kind": DEFERRED_CARD_CHARACTERISTIC,
                    "cardInstanceId": card.instance_id,
                    "characteristic": characteristic,
                })
            };
            let values = [
                (
                    "bindPowerAs",
                    printed_number(definition.power.as_deref())
                        .map(Value::from)
                        .unwrap_or_else(|| deferred("power")),
                ),
                (
                    "bindToughnessAs",
                    printed_number(definition.toughness.as_deref())
                        .map(Value::from)
                        .unwrap_or_else(|| deferred("toughness")),
                ),
                ("bindManaValueAs", Value::from(mana_value(definition))),
            ];
            for (field, value) in values {
                if let Some(binding) = cost[field].as_str() {
                    decisions.insert(binding.to_string(), value);
                }
            }
        }
    }

    /// The current value of a deferred `*` characteristic read: the card's
    /// characteristic-defining power or toughness where it now is, outside
    /// the battlefield (CR 604.3). `None` when the value is not a deferred
    /// read, the card is gone, or no supported ability defines the value.
    pub(super) fn deferred_card_characteristic(&self, value: &Value) -> Option<i32> {
        if value_kind(value) != Some(DEFERRED_CARD_CHARACTERISTIC) {
            return None;
        }
        let card = self.card_outside_battlefield(value["cardInstanceId"].as_str()?)?;
        let (power, toughness) = self.characteristic_defining_power_toughness(card)?;
        let defined = |printed: Option<&str>| printed_number(printed).is_none();
        match value["characteristic"].as_str()? {
            "power" if defined(card.definition.power.as_deref()) => Some(power),
            "toughness" if defined(card.definition.toughness.as_deref()) => Some(toughness),
            _ => None,
        }
    }
}
