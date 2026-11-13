//! Casting-cost selection shared by declaration legality and mana payment.
//!
//! The selected, unresolved mana cost is kept separate from payment so rules
//! that constrain the declared value of X can inspect whether the chosen cost
//! actually contains X before symbols are substituted.

use super::*;

fn remove_generic_mana_from_components(costs: &mut [String], amount: i32) {
    let mut remaining = amount.max(0);
    for cost in costs {
        if remaining == 0 {
            break;
        }
        let removable = mana_cost_generic_amount(cost);
        *cost = reduce_generic_mana_cost(cost, remaining);
        remaining = remaining.saturating_sub(removable);
    }
}

fn finalize_casting_mana_components(
    mut costs: Vec<String>,
    x_value: i64,
    generic_reduction: i32,
    minimum_mana: i32,
) -> Vec<String> {
    let resolved_x = x_value.max(0);
    for cost in &mut costs {
        *cost = cost.replace("{X}", &format!("{{{resolved_x}}}"));
    }

    remove_generic_mana_from_components(&mut costs, generic_reduction);

    let current_mana = costs.iter().fold(0_i32, |total, cost| {
        total.saturating_add(mana_cost_mana_amount(cost))
    });
    let floor = minimum_mana.max(0);
    if current_mana < floor {
        costs.push(format!("{{{}}}", floor - current_mana));
    }
    costs
}

impl GameEngine {
    pub(super) fn selected_alternative_cost(
        &self,
        card: &CardDefinition,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Value> {
        decisions
            .get("effectiveAlternativeCost")
            .filter(|ability| alternative_cost_supported(ability))
            .cloned()
            .or_else(|| {
                card.rules.iter().find_map(|rule| {
                    (value_kind(rule) == Some("keywordAbility")
                        && alternative_cost_supported(&rule["ability"]))
                    .then(|| rule["ability"].clone())
                })
            })
    }

    pub(super) fn selected_casting_base_mana_cost(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> String {
        let bestow_cost = decisions
            .get("useBestow")
            .and_then(Value::as_bool)
            .filter(|paid| *paid)
            .and_then(|_| decisions.get("bestowManaCost"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let free_from_hand = decisions
            .get("castSourceZone")
            .and_then(Value::as_str)
            .unwrap_or("hand")
            == "hand"
            && self
                .battlefield_static_modifiers()
                .iter()
                .any(|(source, modifier)| {
                    (source.controller == controller_id
                        && value_kind(modifier) == Some("castHandSpellsWithoutPayingManaCost"))
                        || (value_kind(modifier) == Some("castingPermission")
                            && modifier["sourceZone"].as_str() == Some("hand")
                            && modifier["withoutPayingManaCost"].as_bool() == Some(true)
                            && match value_kind(&modifier["players"]) {
                                Some("eachPlayer") => true,
                                Some("opponentsOf") => source.controller != controller_id,
                                Some("controllerOf") => source.controller == controller_id,
                                _ => false,
                            }
                            && matches_card_filter(card, &modifier["where"]))
                });

        if decisions
            .get("useAlternativePlayerCounterCost")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || decisions
                .get("useAshlingEvoke")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || decisions
                .get("useSacrificeAlternativeCost")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || decisions
                .get("useAlternativeCost")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || decisions
                .get("withoutPayingManaCost")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            String::new()
        } else if decisions
            .get("awakened")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            card.rules
                .iter()
                .find(|rule| {
                    value_kind(rule) == Some("keywordAbility")
                        && awaken_ability_supported(&rule["ability"])
                })
                .and_then(|rule| rule["ability"]["cost"]["manaCost"].as_str())
                .unwrap_or(&card.mana_cost)
                .to_string()
        } else if decisions
            .get("useForetell")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            decisions
                .get("foretellManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string()
        } else if decisions
            .get("useImpending")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            impending_mana_cost(card).unwrap_or_default().to_string()
        } else if free_from_hand {
            String::new()
        } else if let Some(bestow_cost) = bestow_cost {
            bestow_cost
        } else if decisions
            .get("useEscape")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            escape_mana_cost(card).unwrap_or_default().to_string()
        } else if decisions
            .get("useMayhem")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            mayhem_mana_cost(card)
                .unwrap_or(&card.mana_cost)
                .to_string()
        } else if decisions
            .get("useFlashback")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            flashback_mana_cost(card).unwrap_or_default().to_string()
        } else if let Some(door_index) = decisions
            .get("roomDoor")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
        {
            room_mana_costs(card)
                .get(door_index)
                .cloned()
                .unwrap_or_else(|| card.mana_cost.clone())
        } else {
            card.mana_cost.clone()
        }
    }

    fn casting_generic_reduction(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> i32 {
        card.rules
            .iter()
            .filter(|rule| casting_static_rule_supported(rule))
            .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
            .filter(|modifier| value_kind(modifier) == Some("reduceOwnGenericCastingCost"))
            .filter(|modifier| modifier.get("targetWhere").is_none())
            .filter(|modifier| {
                modifier.get("condition").is_none_or(|condition| {
                    self.condition_value(condition, controller_id, decisions)
                })
            })
            .filter_map(|modifier| self.numeric_expression(&modifier["amount"], controller_id))
            .fold(0_i32, i32::saturating_add)
            .saturating_add(
                self.global_casting_cost_reduction(
                    controller_id,
                    card,
                    decisions
                        .get("castSourceZone")
                        .and_then(Value::as_str)
                        .unwrap_or("hand"),
                ),
            )
            .max(0)
    }

    pub(super) fn casting_mana_costs_for_base_components(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        mut base_components: Vec<String>,
        caller_increases: Vec<String>,
        caller_generic_reduction: i32,
    ) -> Vec<String> {
        base_components.extend(self.casting_additional_mana_costs(card, controller_id, decisions));
        base_components.extend(
            decisions
                .get("spliceManaCosts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned),
        );
        base_components.extend(caller_increases);

        let mut finalized = finalize_casting_mana_components(
            base_components,
            decisions.get("xValue").and_then(Value::as_i64).unwrap_or(0),
            self.casting_generic_reduction(card, controller_id, decisions)
                .saturating_add(caller_generic_reduction.max(0)),
            self.minimum_casting_mana(controller_id, card),
        );
        let delve_payment = if card.rules.iter().any(|rule| {
            value_kind(rule) == Some("keywordAbility")
                && value_kind(&rule["ability"]) == Some("delve")
        }) {
            decisions
                .get("delveCards")
                .and_then(Value::as_array)
                .map(|cards| i32::try_from(cards.len()).unwrap_or(i32::MAX))
                .unwrap_or(0)
        } else {
            0
        };
        remove_generic_mana_from_components(&mut finalized, delve_payment);
        finalized
    }

    pub(super) fn declared_x_is_legal_for_selected_casting_cost(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        let declared_x = decisions.get("xValue").and_then(Value::as_i64).unwrap_or(0);
        if declared_x <= 0 || !card.mana_cost.contains("{X}") {
            return true;
        }
        if self
            .selected_casting_base_mana_cost(card, controller_id, decisions)
            .contains("{X}")
        {
            return true;
        }
        if self
            .casting_additional_mana_costs(card, controller_id, decisions)
            .iter()
            .any(|mana_cost| mana_cost.contains("{X}"))
        {
            return true;
        }
        if decisions.get("useAlternativeCost").and_then(Value::as_bool) != Some(true) {
            return false;
        }
        let selected = self.selected_alternative_cost(card, decisions);
        let selected_mana_costs = selected
            .as_ref()
            .into_iter()
            .flat_map(|ability| ability["costs"].as_array().into_iter().flatten())
            .filter(|cost| value_kind(cost) == Some("payMana"))
            .filter_map(|cost| cost["manaCost"].as_str())
            .collect::<Vec<_>>();
        let selected_cost_contains_x = if selected_mana_costs.is_empty() {
            decisions
                .get("alternativeManaCost")
                .and_then(Value::as_str)
                .is_some_and(|mana_cost| mana_cost.contains("{X}"))
        } else {
            selected_mana_costs
                .iter()
                .any(|mana_cost| mana_cost.contains("{X}"))
        };
        selected_cost_contains_x
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "casting_cost_tests.rs"]
mod tests;
