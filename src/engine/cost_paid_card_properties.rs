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
//! defining `*`) records nothing, and the read stays unbound rather than
//! zero.
use super::*;

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
            let values = [
                ("bindPowerAs", printed_number(definition.power.as_deref())),
                (
                    "bindToughnessAs",
                    printed_number(definition.toughness.as_deref()),
                ),
                ("bindManaValueAs", Some(mana_value(definition))),
            ];
            for (field, value) in values {
                if let (Some(binding), Some(value)) = (cost[field].as_str(), value) {
                    decisions.insert(binding.to_string(), Value::from(value));
                }
            }
        }
    }
}
