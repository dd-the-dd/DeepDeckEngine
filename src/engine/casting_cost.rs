//! Casting-cost selection shared by declaration legality and mana payment.
//!
//! The selected, unresolved mana cost is kept separate from payment so rules
//! that constrain the declared value of X can inspect whether the chosen cost
//! actually contains X before symbols are substituted.

use super::*;

/// The base cost a casting declaration selects (CR 601.2b, 118.9). Spell
/// extras (kicker, splice, replicate, floors) are deliberately outside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CastingBase {
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

/// A step the cast records on the spell once it is on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CastingFollowUp {
    /// "If its alternative cost was paid" checks read
    /// `castWithAlternativeCost:<mode>`.
    MarkAlternativeCost(String),
    /// CR 702.33d: "if it was kicked".
    MarkKicked,
}

/// The casting mode one declaration selected, as data:
/// `castingMode{fromZone, base, additionalCosts, followUps}`. Cost and
/// stack-time readers classify a declaration through this value instead of
/// re-reading individual decision flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CastingMode {
    pub from_zone: String,
    pub base: CastingBase,
    /// Mana components the mode adds to its selected base.
    pub additional_costs: Vec<String>,
    pub follow_ups: Vec<CastingFollowUp>,
}

impl CastingMode {
    pub(super) fn of(decisions: &BTreeMap<String, Value>) -> Self {
        let chosen = |key: &str| decisions.get(key).and_then(Value::as_bool) == Some(true);
        let base = if chosen("useAlternativePlayerCounterCost")
            || chosen("useSacrificeAlternativeCost")
            || chosen("useAlternativeCost")
            || chosen("withoutPayingManaCost")
        {
            CastingBase::Waived
        } else if chosen("awakened") {
            CastingBase::Awaken
        } else if chosen("useForetell") {
            CastingBase::Foretell
        } else if chosen("useImpending") {
            CastingBase::Impending
        } else if chosen("useBestow")
            && decisions
                .get("bestowManaCost")
                .and_then(Value::as_str)
                .is_some()
        {
            CastingBase::Bestow
        } else if chosen("useEscape") {
            CastingBase::Escape
        } else if chosen("useMayhem") {
            CastingBase::Mayhem
        } else if chosen("useFlashback") {
            CastingBase::Flashback
        } else if chosen("miracleCast") {
            CastingBase::Miracle
        } else if let Some(index) = decisions
            .get("roomDoor")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
        {
            CastingBase::Room(index)
        } else {
            CastingBase::Printed
        };
        let mut follow_ups = Vec::new();
        if chosen("useAlternativeCost")
            && let Some(mode) = decisions.get("alternativeCostMode").and_then(Value::as_str)
        {
            follow_ups.push(CastingFollowUp::MarkAlternativeCost(mode.to_string()));
        }
        if chosen("kicked") {
            follow_ups.push(CastingFollowUp::MarkKicked);
        }
        Self {
            from_zone: decisions
                .get("castSourceZone")
                .and_then(Value::as_str)
                .unwrap_or("hand")
                .to_string(),
            base,
            additional_costs: Vec::new(),
            follow_ups,
        }
    }

    /// Whether the printed (or Room door) mana cost is the base, so a
    /// conditional alternative or a permission's mana route may replace it.
    pub(super) fn uses_ordinary_base(&self) -> bool {
        matches!(self.base, CastingBase::Printed | CastingBase::Room(_))
    }

    /// Flags the spell carries on the stack for later "if it was" checks.
    pub(super) fn follow_up_flags(&self) -> impl Iterator<Item = String> + '_ {
        self.follow_ups.iter().map(|follow_up| match follow_up {
            CastingFollowUp::MarkAlternativeCost(mode) => {
                format!("castWithAlternativeCost:{mode}")
            }
            CastingFollowUp::MarkKicked => "kicked".to_string(),
        })
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

/// One colored reduction of a spell's total cost (CR 118.7b-c): `amount`
/// mana of `symbol` (`W`, `U`, `B`, `R`, `G` or `C`). The part exceeding the
/// cost's component of that type reduces its generic component unless the
/// rule says it reduces only colored mana.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColoredCastingReduction {
    pub symbol: String,
    pub amount: i32,
    pub excess_reduces_generic: bool,
}

const COLORED_REDUCTION_SYMBOLS: [&str; 6] = ["W", "U", "B", "R", "G", "C"];

/// Shape of a reduction modifier's `coloredMana` field: the symbols of one
/// delta, how many times it applies, and the excess policy.
/// `castOrdinal`: a later spell of the caster's turn (the first one is
/// `firstEachTurn`), CR 601.2i.
pub(super) fn cast_ordinal_supported(ordinal: &Value) -> bool {
    ordinal.as_u64().is_some_and(|ordinal| (2..=10).contains(&ordinal))
}

pub(super) fn colored_reduction_supported(reduction: &Value) -> bool {
    reduction.as_object().is_some_and(|fields| {
        fields
            .keys()
            .all(|key| matches!(key.as_str(), "symbols" | "times" | "excessReducesGeneric"))
    }) && reduction["symbols"].as_array().is_some_and(|symbols| {
        !symbols.is_empty()
            && symbols.iter().all(|symbol| {
                symbol
                    .as_str()
                    .is_some_and(|symbol| COLORED_REDUCTION_SYMBOLS.contains(&symbol))
            })
    }) && numeric_expression_supported(&reduction["times"])
        && reduction["excessReducesGeneric"].is_boolean()
}

/// The colored reductions one `coloredMana` field gives when its delta
/// applies `times` times.
pub(super) fn colored_casting_reductions(
    reduction: &Value,
    times: i32,
) -> Result<Vec<ColoredCastingReduction>, EngineError> {
    if !colored_reduction_supported(reduction) {
        return Err(EngineError::new("unsupported colored casting reduction"));
    }
    let times = times.max(0);
    let excess_reduces_generic = reduction["excessReducesGeneric"].as_bool() == Some(true);
    let mut counts = BTreeMap::<String, i32>::new();
    for symbol in reduction["symbols"].as_array().into_iter().flatten() {
        let symbol = symbol.as_str().unwrap_or_default().to_string();
        let count = counts.entry(symbol).or_default();
        *count = count
            .checked_add(times)
            .ok_or_else(|| EngineError::new("colored casting reduction exceeds i32"))?;
    }
    Ok(counts
        .into_iter()
        .filter(|(_, amount)| *amount > 0)
        .map(|(symbol, amount)| ColoredCastingReduction {
            symbol,
            amount,
            excess_reduces_generic,
        })
        .collect())
}

/// Removes each colored reduction from the cost components (CR 118.7b-c)
/// and returns the generic reduction its excess adds. A component symbol
/// that is hybrid or Phyrexian in the reduced color has no rule here and is
/// refused rather than guessed.
fn apply_colored_casting_reductions(
    costs: &mut [String],
    reductions: &[ColoredCastingReduction],
) -> Result<i32, EngineError> {
    let mut excess = 0_i32;
    for reduction in reductions {
        let mut remaining = reduction.amount;
        for cost in costs.iter_mut() {
            if remaining == 0 {
                break;
            }
            let mut symbols = parse_mana_symbols(cost);
            if symbols.iter().any(|symbol| {
                symbol.contains('/') && symbol.split('/').any(|part| part == reduction.symbol)
            }) {
                return Err(EngineError::new(
                    "a colored cost reduction of a hybrid or Phyrexian symbol is unsupported",
                ));
            }
            let before = symbols.len();
            symbols.retain(|symbol| {
                if remaining > 0 && *symbol == reduction.symbol {
                    remaining -= 1;
                    false
                } else {
                    true
                }
            });
            if symbols.len() != before {
                *cost = symbols
                    .iter()
                    .map(|symbol| format!("{{{symbol}}}"))
                    .collect::<String>();
            }
        }
        if reduction.excess_reduces_generic {
            excess = excess
                .checked_add(remaining)
                .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?;
        }
    }
    Ok(excess)
}

/// `This spell costs COST more to cast for each target beyond the first`
/// (CR 601.2f; strive, CR 702.108): a fixed mana increase of the spell's own
/// total cost, counted on the targets chosen before that cost (CR 601.2c).
pub(super) fn own_casting_increase_supported(modifier: &Value) -> bool {
    value_kind(modifier) == Some("increaseOwnCastingCost")
        && modifier.as_object().is_some_and(|fields| {
            fields
                .keys()
                .all(|key| matches!(key.as_str(), "kind" | "manaCost" | "per"))
        })
        && modifier["manaCost"]
            .as_str()
            .is_some_and(|cost| parse_fixed_mana_requirement_checked(cost).is_some())
        && modifier["per"].as_str() == Some("targetBeyondFirst")
}

/// The increases a spell's own `increaseOwnCastingCost` rules add for the
/// targets of one casting declaration: one copy of the cost for each chosen
/// target after the first.
pub(super) fn own_target_count_increases(
    card: &CardDefinition,
    targets: &BTreeMap<String, TargetRef>,
) -> Vec<String> {
    let beyond_first = targets.len().saturating_sub(1);
    card.printed_rules()
        .iter()
        .filter(|rule| casting_static_rule_supported(rule))
        .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
        .filter(|modifier| own_casting_increase_supported(modifier))
        .filter_map(|modifier| modifier["manaCost"].as_str())
        .flat_map(|cost| std::iter::repeat_n(cost.to_string(), beyond_first))
        .collect()
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
        base_components.extend(CastingMode::of(decisions).additional_costs);
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
        if !card.printed_rules().iter().any(|rule| {
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
                card.printed_rules().iter().find_map(|rule| {
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
        match CastingMode::of(decisions).base {
            CastingBase::Waived => String::new(),
            CastingBase::Awaken => card
                .printed_rules()
                .iter()
                .find(|rule| {
                    value_kind(rule) == Some("keywordAbility")
                        && awaken_ability_supported(&rule["ability"])
                })
                .and_then(|rule| rule["ability"]["cost"]["manaCost"].as_str())
                .unwrap_or(&card.mana_cost)
                .to_string(),
            CastingBase::Foretell => decisions
                .get("foretellManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            CastingBase::Impending => impending_mana_cost(card).unwrap_or_default().to_string(),
            CastingBase::Bestow => decisions
                .get("bestowManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            CastingBase::Escape => escape_mana_cost(card).unwrap_or_default().to_string(),
            CastingBase::Mayhem => mayhem_mana_cost(card)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            CastingBase::Flashback => flashback_mana_cost(card).unwrap_or_default().to_string(),
            CastingBase::Miracle => decisions
                .get("miracleManaCost")
                .and_then(Value::as_str)
                .unwrap_or(&card.mana_cost)
                .to_string(),
            CastingBase::Room(index) => room_mana_costs(card)
                .get(index)
                .cloned()
                .unwrap_or_else(|| card.mana_cost.clone()),
            CastingBase::Printed => card.mana_cost.clone(),
        }
    }

    /// The generic reduction total and the colored reductions (CR 118.7b-c)
    /// of the spell's own and global cost-reduction rules (CR 601.2f).
    fn checked_casting_reductions(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
    ) -> Result<(i32, Vec<ColoredCastingReduction>), EngineError> {
        let context =
            cost_query::CastingQueryContext::spell(card, controller_id, decisions, casting_source);
        let mut total = 0_i32;
        let mut colored = Vec::new();
        for modifier in card
            .printed_rules()
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
            if let Some(reduction) = modifier.get("coloredMana") {
                let times = self.checked_cost_query_numeric(
                    &reduction["times"],
                    &context,
                    "own colored casting reduction multiplier",
                )?;
                colored.extend(colored_casting_reductions(reduction, times)?);
            }
        }
        let (global, global_colored) = self.checked_global_casting_cost_reductions(
            controller_id,
            card,
            decisions
                .get("castSourceZone")
                .and_then(Value::as_str)
                .unwrap_or("hand"),
            decisions,
            casting_source,
        )?;
        colored.extend(global_colored);
        let generic = total
            .checked_add(global)
            .and_then(|total| {
                total.checked_add(keyword_programs::alternative_cost_generic_reduction(
                    decisions,
                ))
            })
            .map(|amount| amount.max(0))
            .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?;
        Ok((generic, colored))
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

        // CR 601.2f: base or alternative plus increases, then reductions
        // (colored first, CR 118.7b-c spilling into generic), then floors;
        // Delve is a payment substitution applied after the total is locked.
        let (generic_reduction, colored_reductions) =
            self.checked_casting_reductions(card, controller_id, decisions, casting_source)?;
        let colored_excess =
            apply_colored_casting_reductions(&mut base_components, &colored_reductions)?;
        let mut finalized = finalize_casting_mana_components(
            base_components,
            decisions.get("xValue").and_then(Value::as_i64).unwrap_or(0),
            generic_reduction
                .checked_add(caller_generic_reduction.max(0))
                .and_then(|total| total.checked_add(colored_excess))
                .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?,
            self.minimum_casting_mana(controller_id, card),
        );
        let delve_payment = if card.printed_rules().iter().any(|rule| {
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "cost_modification_x_tests.rs"]
mod cost_modification_x_tests;
