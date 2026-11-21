//! Casting-cost selection shared by declaration legality and mana payment.
//!
//! The selected, unresolved mana cost is kept separate from payment so rules
//! that constrain the declared value of X can inspect whether the chosen cost
//! actually contains X before symbols are substituted.

use super::*;

/// Sole declaration-level classifier for choosing a base cost. Spell extras
/// (kicker, splice, replicate, floors) are deliberately outside this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedCastingBasePolicy {
    Printed,
    Waived,
    Awaken,
    Foretell,
    Impending,
    Bestow,
    Escape,
    Mayhem,
    Flashback,
    Miracle,
    Room(usize),
}
pub(super) fn selected_casting_base_policy(
    decisions: &BTreeMap<String, Value>,
) -> SelectedCastingBasePolicy {
    let chosen = |key: &str| decisions.get(key).and_then(Value::as_bool) == Some(true);
    if chosen("useAlternativePlayerCounterCost")
        || chosen("useAshlingEvoke")
        || chosen("useSacrificeAlternativeCost")
        || chosen("useAlternativeCost")
        || chosen("withoutPayingManaCost")
    {
        SelectedCastingBasePolicy::Waived
    } else if chosen("awakened") {
        SelectedCastingBasePolicy::Awaken
    } else if chosen("useForetell") {
        SelectedCastingBasePolicy::Foretell
    } else if chosen("useImpending") {
        SelectedCastingBasePolicy::Impending
    } else if chosen("useBestow")
        && decisions
            .get("bestowManaCost")
            .and_then(Value::as_str)
            .is_some()
    {
        SelectedCastingBasePolicy::Bestow
    } else if chosen("useEscape") {
        SelectedCastingBasePolicy::Escape
    } else if chosen("useMayhem") {
        SelectedCastingBasePolicy::Mayhem
    } else if chosen("useFlashback") {
        SelectedCastingBasePolicy::Flashback
    } else if chosen("miracleCast") {
        SelectedCastingBasePolicy::Miracle
    } else if let Some(index) = decisions
        .get("roomDoor")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
    {
        SelectedCastingBasePolicy::Room(index)
    } else {
        SelectedCastingBasePolicy::Printed
    }
}

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
    pub(super) fn checked_casting_mana_costs(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
    ) -> Result<Vec<String>, EngineError> {
        self.checked_casting_mana_costs_with_adjustments(
            card,
            controller_id,
            decisions,
            casting_source,
            Vec::new(),
            0,
        )
    }

    pub(super) fn checked_casting_mana_costs_with_adjustments(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
        caller_increases: Vec<String>,
        caller_generic_reduction: i32,
    ) -> Result<Vec<String>, EngineError> {
        let mut base_components =
            vec![self.selected_casting_base_mana_cost(card, controller_id, decisions)];
        if decisions.get("useAshlingEvoke").and_then(Value::as_bool) == Some(true) {
            base_components.push("{4}".to_string());
        }
        if decisions.get("useAlternativeCost").and_then(Value::as_bool) == Some(true) {
            let selected_alternative = self.selected_alternative_cost(card, decisions);
            let selected_mana_costs = selected_alternative
                .as_ref()
                .into_iter()
                .flat_map(|ability| ability["costs"].as_array().into_iter().flatten())
                .filter(|cost| value_kind(cost) == Some("payMana"))
                .filter_map(|cost| cost["manaCost"].as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>();
            let selected_has_mana_cost = !selected_mana_costs.is_empty();
            base_components.extend(selected_mana_costs);
            if !selected_has_mana_cost
                && let Some(cost) = decisions.get("alternativeManaCost").and_then(Value::as_str)
            {
                base_components.push(cost.to_string());
            }
        }
        self.checked_casting_mana_costs_for_base_components(
            card,
            controller_id,
            decisions,
            casting_source,
            base_components,
            caller_increases,
            caller_generic_reduction,
        )
    }

    #[cfg(test)]
    pub(super) fn casting_mana_costs(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        self.checked_casting_mana_costs(card, controller_id, decisions, None)
            .expect("historical cost fixture must have a supported checked query")
    }
    #[cfg(test)]
    fn casting_mana_costs_with_adjustments(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        increases: Vec<String>,
        reduction: i32,
    ) -> Vec<String> {
        self.checked_casting_mana_costs_with_adjustments(
            card,
            controller_id,
            decisions,
            None,
            increases,
            reduction,
        )
        .expect("historical cost fixture must have a supported checked query")
    }
    #[cfg(test)]
    fn casting_mana_costs_for_base_components(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        base: Vec<String>,
        increases: Vec<String>,
        reduction: i32,
    ) -> Vec<String> {
        self.checked_casting_mana_costs_for_base_components(
            card,
            controller_id,
            decisions,
            None,
            base,
            increases,
            reduction,
        )
        .expect("historical cost fixture must have a supported checked query")
    }

    /// Validate and lock the selected generic payment substitution while the
    /// real source is still admitted in the world, before ANY payment mutates it.
    pub(super) fn checked_delve_payment_maximum(
        &self,
        action: &LegalAction,
        card: &CardDefinition,
        casting_source: Option<&CardInstance>,
    ) -> Result<usize, EngineError> {
        let Some(value) = action.decisions.get("delveCards") else {
            return Ok(0);
        };
        let selected = value
            .as_array()
            .ok_or_else(|| EngineError::new("delve selection must be an array"))?;
        if selected.is_empty() {
            return Ok(0);
        }
        if !card.rules.iter().any(|rule| {
            value_kind(rule) == Some("keywordAbility")
                && value_kind(&rule["ability"]) == Some("delve")
        }) {
            return Err(EngineError::new(
                "delve cards were selected for a spell without delve",
            ));
        }
        let ids = selected
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| EngineError::new("delve selection must contain card identities"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len() {
            return Err(EngineError::new("the same card cannot pay for delve twice"));
        }
        let player = self.player_index(&action.player_id)?;
        if ids.iter().any(|id| {
            casting_source.is_some_and(|source| source.instance_id == *id)
                || !self.state.players[player]
                    .graveyard
                    .iter()
                    .any(|card| card.instance_id == *id)
        }) {
            return Err(EngineError::new(
                "delve requires cards in the caster's graveyard other than the casting spell",
            ));
        }
        let mut decisions = action.decisions.clone();
        decisions.remove("delveCards");
        let costs =
            self.checked_casting_mana_costs(card, &action.player_id, &decisions, casting_source)?;
        let maximum = costs.iter().try_fold(0_usize, |total, cost| {
            total
                .checked_add(mana_cost_generic_amount(cost).max(0) as usize)
                .ok_or_else(|| EngineError::new("delve maximum exceeds usize"))
        })?;
        if ids.len() > maximum {
            return Err(EngineError::new(
                "delve cannot pay more than the spell's generic mana cost",
            ));
        }
        Ok(maximum)
    }

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
        _controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> String {
        match selected_casting_base_policy(decisions) {
            SelectedCastingBasePolicy::Waived => String::new(),
            SelectedCastingBasePolicy::Awaken => card
                .rules
                .iter()
                .find(|rule| {
                    value_kind(rule) == Some("keywordAbility")
                        && awaken_ability_supported(&rule["ability"])
                })
                .and_then(|rule| rule["ability"]["cost"]["manaCost"].as_str())
                .unwrap_or(&card.mana_cost)
                .to_string(),
            SelectedCastingBasePolicy::Foretell => decisions
                .get("foretellManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            SelectedCastingBasePolicy::Impending => {
                impending_mana_cost(card).unwrap_or_default().to_string()
            }
            SelectedCastingBasePolicy::Bestow => decisions
                .get("bestowManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            SelectedCastingBasePolicy::Escape => {
                escape_mana_cost(card).unwrap_or_default().to_string()
            }
            SelectedCastingBasePolicy::Mayhem => mayhem_mana_cost(card)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            SelectedCastingBasePolicy::Flashback => {
                flashback_mana_cost(card).unwrap_or_default().to_string()
            }
            SelectedCastingBasePolicy::Miracle => decisions
                .get("miracleManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            SelectedCastingBasePolicy::Room(index) => room_mana_costs(card)
                .get(index)
                .cloned()
                .unwrap_or_else(|| card.mana_cost.clone()),
            SelectedCastingBasePolicy::Printed => card.mana_cost.clone(),
        }
    }

    fn checked_casting_generic_reduction(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
    ) -> Result<i32, EngineError> {
        let context =
            cost_query::CastingQueryContext::spell(card, controller_id, decisions, casting_source);
        let mut total = 0_i32;
        for modifier in card
            .rules
            .iter()
            .filter(|rule| casting_static_rule_supported(rule))
            .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
        {
            if value_kind(modifier) != Some("reduceOwnGenericCastingCost")
                || modifier.get("targetWhere").is_some()
            {
                continue;
            }
            if modifier
                .get("condition")
                .is_some_and(|condition| !self.condition_value(condition, controller_id, decisions))
            {
                continue;
            }
            let amount = self.checked_cost_query_numeric(
                &modifier["amount"],
                &context,
                "own casting reduction amount",
            )?;
            total = total
                .checked_add(amount)
                .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?;
        }
        total
            .checked_add(
                self.checked_global_casting_cost_reduction(
                    controller_id,
                    card,
                    decisions
                        .get("castSourceZone")
                        .and_then(Value::as_str)
                        .unwrap_or("hand"),
                    decisions,
                    casting_source,
                )?,
            )
            .map(|amount| amount.max(0))
            .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))
    }

    pub(super) fn checked_casting_mana_costs_for_base_components(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
        mut base_components: Vec<String>,
        caller_increases: Vec<String>,
        caller_generic_reduction: i32,
    ) -> Result<Vec<String>, EngineError> {
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
            self.checked_casting_generic_reduction(card, controller_id, decisions, casting_source)?
                .checked_add(caller_generic_reduction.max(0))
                .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?,
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
        Ok(finalized)
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
