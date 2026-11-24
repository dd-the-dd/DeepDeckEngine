//! Tapping chosen permanents whose total power meets a bound, as an
//! activation cost (CR 118.3, 602.2b).
//!
//! `tapPermanentsWithTotalPower` is the IR of `Tap any number of [other]
//! [untapped] QUERY you control with total power N or greater`: the Crew
//! (CR 702.122a) and Saddle (CR 702.171a) definitions and printed costs of the
//! same shape. The payer declares the tapped set while activating
//! (`decisionId`, an array of permanent instance ids). The set is legal when
//! every member is an untapped permanent the payer controls that matches
//! `where`, is not the source when the source is excluded (`excludeSource`,
//! or a `{T}` cost of the same activation), is not tapped for mana by the same
//! payment, and the members' current powers sum to at least `minimumPower`.
//! Summoning sickness does not restrict this cost: it is not a `{T}` cost
//! (CR 302.6).
//!
//! The same check generalizes the spell-side teamwork payment (selected
//! untapped creatures, summed power): one legality rule for the declared set,
//! read by enumeration, validation and payment.
use super::*;

pub(super) const TOTAL_POWER_TAP_COST: &str = "tapPermanentsWithTotalPower";

/// Schema of the cost; anything else stays unexecutable.
pub(super) fn total_power_tap_cost_supported(cost: &Value) -> bool {
    cost["decisionId"].as_str().is_some_and(|id| !id.is_empty())
        && integer_value(&cost["minimumPower"]).is_some_and(|power| power > 0)
        && permanent_filter_supported(&cost["where"])
        && cost.get("excludeSource").is_none_or(Value::is_boolean)
}

fn total_power_tap_costs(rule: &Value) -> impl Iterator<Item = &Value> {
    rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cost| value_kind(cost) == Some(TOTAL_POWER_TAP_COST))
}

/// The source cannot be tapped by this cost when the cost says `other`, or
/// when a `{T}` cost of the same activation taps it.
fn excludes_source(rule: &Value, cost: &Value) -> bool {
    cost["excludeSource"].as_bool() == Some(true)
        || rule["costs"].as_array().into_iter().flatten().any(|other| {
            value_kind(other) == Some("tap")
                && matches!(value_kind(&other["object"]), Some("self" | "abilitySource"))
        })
}

impl GameEngine {
    /// Untapped permanents `controller_id` controls that may be tapped for
    /// this cost, in battlefield order.
    fn total_power_tap_candidates(
        &self,
        rule: &Value,
        cost: &Value,
        controller_id: &str,
        source_instance_id: &str,
    ) -> Vec<String> {
        let exclude_source = excludes_source(rule, cost);
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| {
                permanent.controller == controller_id
                    && !permanent.tapped
                    && !permanent.flags.get("phasedOut").copied().unwrap_or(false)
                    && !(exclude_source && permanent.instance_id == source_instance_id)
                    && self.permanent_matches_effective_filter(permanent, &cost["where"])
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect()
    }

    fn total_power_of(&self, instance_ids: &[String]) -> i32 {
        instance_ids
            .iter()
            .filter_map(|instance_id| self.permanent_position(instance_id))
            .map(|(player_index, card_index)| {
                self.current_power(&self.state.players[player_index].battlefield[card_index])
            })
            .sum()
    }

    /// Expands each activation declaration with every legal tapped set of
    /// each total-power tap cost, smallest sets first, within the shared
    /// declaration budget. No legal set leaves no declaration: the ability
    /// cannot be activated.
    pub(super) fn expand_total_power_tap_declarations(
        &self,
        rule: &Value,
        controller_id: &str,
        source_instance_id: &str,
        mut declarations: Vec<CastingDeclaration>,
    ) -> Vec<CastingDeclaration> {
        for cost in total_power_tap_costs(rule) {
            let (Some(decision_id), Some(minimum_power)) = (
                cost["decisionId"].as_str(),
                integer_value(&cost["minimumPower"]),
            ) else {
                return Vec::new();
            };
            let candidates =
                self.total_power_tap_candidates(rule, cost, controller_id, source_instance_id);
            let mut selections = Vec::new();
            for count in 1..=candidates.len() {
                if selections.len() >= MAX_DECLARATION_OPTIONS {
                    break;
                }
                selections.extend(
                    Self::resolution_selections(
                        &candidates,
                        count,
                        MAX_DECLARATION_OPTIONS.saturating_sub(selections.len()),
                    )
                    .into_iter()
                    .filter(|selection| self.total_power_of(selection) >= minimum_power),
                );
            }
            let mut expanded = Vec::new();
            'declarations: for declaration in declarations {
                for selection in &selections {
                    if expanded.len() >= MAX_DECLARATION_OPTIONS {
                        break 'declarations;
                    }
                    // A permanent pays one total-power tap cost at most.
                    let already_tapped = declaration.decisions.values().any(|chosen| {
                        chosen.as_array().is_some_and(|chosen| {
                            chosen.iter().any(|id| {
                                id.as_str()
                                    .is_some_and(|id| selection.iter().any(|s| s == id))
                            })
                        })
                    });
                    if already_tapped {
                        continue;
                    }
                    let mut paid = declaration.clone();
                    paid.decisions
                        .insert(decision_id.to_string(), json!(selection));
                    expanded.push(paid);
                }
            }
            declarations = expanded;
        }
        declarations
    }

    /// The declared set of each total-power tap cost is legal now: checked
    /// before any cost of the activation is paid (CR 601.2h via 602.2b).
    pub(super) fn validate_total_power_tap_costs(
        &self,
        action: &LegalAction,
        rule: &Value,
        source_instance_id: &str,
    ) -> Result<(), EngineError> {
        let mut paid = BTreeSet::new();
        for cost in total_power_tap_costs(rule) {
            let decision_id = cost["decisionId"]
                .as_str()
                .ok_or_else(|| EngineError::new("total-power tap cost has no decision"))?;
            let minimum_power = integer_value(&cost["minimumPower"])
                .ok_or_else(|| EngineError::new("total-power tap cost has no bound"))?;
            let selected = action
                .decisions
                .get(decision_id)
                .and_then(Value::as_array)
                .ok_or_else(|| EngineError::new("total-power tap cost has no tapped permanents"))?
                .iter()
                .map(|id| {
                    id.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                        EngineError::new("a total-power tap choice is not a permanent")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let candidates =
                self.total_power_tap_candidates(rule, cost, &action.player_id, source_instance_id);
            for instance_id in &selected {
                if !paid.insert(instance_id.clone()) {
                    return Err(EngineError::new(
                        "a permanent cannot be tapped twice for total-power tap costs",
                    ));
                }
                if !candidates.contains(instance_id)
                    || action
                        .payment_sources
                        .iter()
                        .any(|payment_source| payment_source == instance_id)
                {
                    return Err(EngineError::new(
                        "a selected permanent cannot be tapped for this cost",
                    ));
                }
            }
            if self.total_power_of(&selected) < minimum_power {
                return Err(EngineError::new(format!(
                    "the tapped permanents need total power {minimum_power} or greater"
                )));
            }
        }
        Ok(())
    }

    /// Taps every permanent of each validated total-power tap set. The
    /// tapped permanents are returned for the caller's tapped triggers.
    pub(super) fn pay_total_power_tap_costs(
        &mut self,
        action: &LegalAction,
        rule: &Value,
    ) -> Result<Vec<CardInstance>, EngineError> {
        let selected = total_power_tap_costs(rule)
            .filter_map(|cost| cost["decisionId"].as_str())
            .flat_map(|decision_id| {
                action
                    .decisions
                    .get(decision_id)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        let mut tapped = Vec::new();
        for instance_id in selected {
            let permanent = self.tap_permanent(&instance_id)?.ok_or_else(|| {
                EngineError::new("a selected permanent was already tapped while paying")
            })?;
            tapped.push(permanent);
        }
        Ok(tapped)
    }
}
