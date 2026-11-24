//! One payment search with an explicit fallible mana-source boundary.
//! Unsupported production and impossible payment are distinct outcomes.
use super::*;

impl GameEngine {
    /// Historical adapter stays independent until the complete Result route
    /// replaces its consumers. Infallible is eliminated exhaustively.
    pub(super) fn payment_for_mana_costs_with_context(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
    ) -> Option<ManaPayment> {
        let mut resolve = |permanent: &CardInstance, spending_card: Option<&CardDefinition>| {
            Ok::<_, std::convert::Infallible>(self.mana_choices_for_permanent(
                permanent,
                spending_card,
                false,
                false,
            ))
        };
        match self.try_plan_mana_payment(
            player_index,
            mana_costs,
            excluded_sources,
            spending_card,
            ability_source,
            spending_source_zone,
            &mut resolve,
        ) {
            Ok(payment) => payment,
            Err(never) => match never {},
        }
    }

    pub(super) fn try_plan_mana_payment<E, F>(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        resolve: &mut F,
    ) -> Result<Option<ManaPayment>, E>
    where
        F: FnMut(&CardInstance, Option<&CardDefinition>) -> Result<Vec<ManaChoice>, E>,
    {
        let started = Instant::now();
        let mut requirement = ManaRequirement::default();
        for mana_cost in mana_costs {
            let Some(parsed) = parse_mana_requirement(mana_cost) else {
                return Ok(None);
            };
            requirement.add(&parsed);
        }
        self.try_plan_mana_requirement(
            player_index,
            requirement,
            excluded_sources,
            spending_card,
            ability_source,
            spending_source_zone,
            resolve,
            started,
        )
    }

    /// Shared typed search kernel. The start instant keeps the legacy string
    /// adapter's existing budget boundary; typed callers start at this search.
    pub(super) fn try_plan_mana_requirement<E, F>(
        &self,
        player_index: usize,
        requirement: ManaRequirement,
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        resolve: &mut F,
        started: Instant,
    ) -> Result<Option<ManaPayment>, E>
    where
        F: FnMut(&CardInstance, Option<&CardDefinition>) -> Result<Vec<ManaChoice>, E>,
    {
        if requirement.is_empty() {
            return Ok(Some(ManaPayment::default()));
        }
        let player = &self.state.players[player_index];
        let eligible_pool_indices = player
            .mana_pool
            .iter()
            .enumerate()
            .filter(|(_, mana)| {
                mana_postcost_ordered::mana_pool_entry_can_pay(
                    mana,
                    spending_card,
                    ability_source,
                    spending_source_zone,
                )
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let mut pool_indices = Vec::new();
        let mut mana = Vec::new();
        if requirement.snow == 0 {
            for (symbol, required) in &requirement.colored {
                let matching_pool_indices = eligible_pool_indices
                    .iter()
                    .copied()
                    .filter(|pool_index| {
                        !pool_indices.contains(pool_index)
                            && player.mana_pool[*pool_index].symbol == *symbol
                    })
                    .take((*required).max(0) as usize)
                    .collect::<Vec<_>>();
                for pool_index in matching_pool_indices {
                    pool_indices.push(pool_index);
                    mana.push((
                        player.mana_pool[pool_index].symbol.clone(),
                        player.mana_pool[pool_index]
                            .spend_restriction
                            .as_ref()
                            .and_then(|restriction| restriction["producedBySnowSource"].as_bool())
                            == Some(true),
                    ));
                }
            }
            let generic_pool_indices = eligible_pool_indices
                .iter()
                .copied()
                .filter(|pool_index| !pool_indices.contains(pool_index))
                .take(requirement.generic.max(0) as usize)
                .collect::<Vec<_>>();
            for pool_index in generic_pool_indices {
                pool_indices.push(pool_index);
                mana.push((
                    player.mana_pool[pool_index].symbol.clone(),
                    player.mana_pool[pool_index]
                        .spend_restriction
                        .as_ref()
                        .and_then(|restriction| restriction["producedBySnowSource"].as_bool())
                        == Some(true),
                ));
            }
        }
        let mut sources = Vec::new();
        for permanent in self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|permanent| {
                !(excluded_sources.contains(&permanent.instance_id)
                    || self.permanent_cannot_pay_tap_symbol(permanent))
            })
        {
            let choices = resolve(permanent, spending_card)?
                .into_iter()
                .filter(|choice| {
                    choice.spend_restriction.as_ref().is_none_or(|restriction| {
                        value_kind(restriction) != Some("castSpellFromZone")
                            || spending_source_zone == restriction["zone"].as_str()
                    })
                })
                .collect::<Vec<_>>();
            if !choices.is_empty() {
                sources.push((permanent.instance_id.clone(), choices));
            }
        }
        sources.sort_by_key(|(source_id, choices)| {
            let permanent = self.state.players[player_index]
                .battlefield
                .iter()
                .find(|candidate| candidate.instance_id == *source_id)
                .expect("mana source came from the battlefield");
            let restricted_for_spell = choices
                .iter()
                .any(|choice| choice.spend_restriction.is_some());
            let produces_only_colorless = choices.iter().all(|choice| {
                !choice.symbols.is_empty() && choice.symbols.iter().all(|symbol| symbol == "C")
            });
            let is_basic = permanent
                .definition
                .type_line
                .to_ascii_lowercase()
                .contains("basic land");
            let is_multicolor = choices
                .iter()
                .flat_map(|choice| choice.symbols.iter())
                .filter(|symbol| ["W", "U", "B", "R", "G"].contains(&symbol.as_str()))
                .collect::<BTreeSet<_>>()
                .len()
                > 1;
            let animated_land =
                is_land(&permanent.definition) && self.permanent_is_creature(permanent);
            let priority = if animated_land {
                5
            } else if restricted_for_spell {
                0
            } else if produces_only_colorless {
                1
            } else if is_basic {
                2
            } else if is_multicolor {
                4
            } else {
                3
            };
            (priority, source_id.clone())
        });

        let pool_options = eligible_pool_indices
            .iter()
            .map(|index| {
                (
                    *index,
                    player.mana_pool[*index].symbol.clone(),
                    player.mana_pool[*index]
                        .spend_restriction
                        .as_ref()
                        .and_then(|restriction| restriction["producedBySnowSource"].as_bool())
                        == Some(true),
                )
            })
            .collect::<Vec<_>>();
        let mut used_pool = pool_options
            .iter()
            .map(|(index, _, _)| pool_indices.contains(index))
            .collect::<Vec<_>>();
        let mut used = vec![false; sources.len()];
        let mut selected = Vec::new();
        let mut phyrexian_life_paid = 0;
        let mut failed_states = BTreeSet::new();
        let found = search_payment(
            &pool_options,
            &sources,
            &requirement,
            &mut used_pool,
            &mut used,
            &mut pool_indices,
            &mut selected,
            &mut mana,
            self.state.players[player_index].life,
            &mut phyrexian_life_paid,
            &mut failed_states,
        );
        if selected
            .iter()
            .any(|activation| !activation.activation_requirement.is_empty())
        {
            selected.reverse();
        }
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(250) {
            eprintln!(
                "[engine-slow] mana-payment elapsedMs={} player={} requirement={:?} card={:?} sources={} choices={} selected={} found={}",
                elapsed.as_millis(),
                player.id,
                requirement,
                spending_card.map(|card| card.name.as_str()),
                sources.len(),
                sources
                    .iter()
                    .map(|(_, choices)| choices.len())
                    .sum::<usize>(),
                selected.len(),
                found,
            );
        }
        let cant_be_countered = selected.iter().any(|activation| {
            sources
                .iter()
                .find(|(source_id, _)| *source_id == activation.source_id)
                .and_then(|(_, choices)| {
                    choices.iter().find(|choice| {
                        choice.origin == activation.origin && choice.symbols == activation.symbols
                    })
                })
                .and_then(|choice| choice.spend_restriction.as_ref())
                .is_some_and(|restriction| restriction["cantBeCountered"].as_bool() == Some(true))
        }) || pool_indices.iter().any(|pool_index| {
            player.mana_pool[*pool_index]
                .spend_restriction
                .as_ref()
                .is_some_and(|restriction| restriction["cantBeCountered"].as_bool() == Some(true))
        });
        Ok(found.then_some(ManaPayment {
            activations: selected,
            phyrexian_life_paid,
            pool_symbols: pool_indices
                .iter()
                .map(|pool_index| player.mana_pool[*pool_index].symbol.clone())
                .collect(),
            pool_indices,
            cant_be_countered,
            ordered: false,
        }))
    }
}

fn missing_colored_symbol(
    mana: &[(String, bool)],
    requirement: &ManaRequirement,
) -> Option<String> {
    requirement.colored.iter().find_map(|(symbol, required)| {
        let available = mana
            .iter()
            .filter(|(candidate, _)| candidate == symbol)
            .count() as i32;
        (available < *required).then(|| symbol.clone())
    })
}

fn search_payment(
    pool_options: &[(usize, String, bool)],
    sources: &[(String, Vec<ManaChoice>)],
    requirement: &ManaRequirement,
    used_pool: &mut [bool],
    used: &mut [bool],
    selected_pool: &mut Vec<usize>,
    selected: &mut Vec<ManaActivation>,
    mana: &mut Vec<(String, bool)>,
    remaining_life: i32,
    phyrexian_life_paid: &mut i32,
    failed_states: &mut BTreeSet<ManaPaymentSearchState>,
) -> bool {
    let available_phyrexian_payment = phyrexian_life_payment(mana, requirement, remaining_life);
    if available_phyrexian_payment == Some(0) {
        *phyrexian_life_paid = 0;
        return true;
    }

    let symbol_counts = mana
        .iter()
        .fold(BTreeMap::new(), |mut counts, (symbol, _)| {
            *counts.entry(symbol.clone()).or_default() += 1;
            counts
        });
    let search_state = ManaPaymentSearchState {
        requirement_colored: requirement.colored.clone(),
        requirement_generic: requirement.generic,
        requirement_hybrid: requirement.hybrid.clone(),
        requirement_phyrexian: requirement.phyrexian.clone(),
        requirement_snow: requirement.snow,
        remaining_life,
        symbols: symbol_counts.clone(),
        snow_mana: mana.iter().filter(|(_, snow)| *snow).count(),
        used_pool: used_pool.to_vec(),
        used_sources: used.to_vec(),
    };
    if failed_states.contains(&search_state) {
        return false;
    }

    let available_pool = pool_options
        .iter()
        .enumerate()
        .filter(|(index, _)| !used_pool[*index])
        .count();
    let available_source_mana = sources
        .iter()
        .enumerate()
        .filter(|(index, _)| !used[*index])
        .map(|(_, (_, choices))| {
            choices
                .iter()
                .filter(|choice| choice.life_cost <= remaining_life)
                .map(|choice| choice.symbols.len())
                .max()
                .unwrap_or(0)
        })
        .sum::<usize>();
    let phyrexian_symbols = requirement
        .phyrexian
        .values()
        .map(|amount| (*amount).max(0))
        .sum::<i32>();
    let phyrexian_mana_required = (phyrexian_symbols - remaining_life.max(0) / 2).max(0);
    let required_mana = requirement.generic.max(0) as usize
        + requirement.snow.max(0) as usize
        + requirement
            .colored
            .values()
            .map(|amount| (*amount).max(0))
            .sum::<i32>() as usize
        + requirement.hybrid.len();
    let required_mana = required_mana + phyrexian_mana_required as usize;
    if mana.len() + available_pool + available_source_mana < required_mana {
        failed_states.insert(search_state);
        return false;
    }
    for (required_symbol, required) in &requirement.colored {
        let available_from_pool = pool_options
            .iter()
            .enumerate()
            .filter(|(index, (_, symbol, _))| !used_pool[*index] && symbol == required_symbol)
            .count();
        let available_from_sources = sources
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .map(|(_, (_, choices))| {
                choices
                    .iter()
                    .filter(|choice| choice.life_cost <= remaining_life)
                    .map(|choice| {
                        choice
                            .symbols
                            .iter()
                            .filter(|symbol| *symbol == required_symbol)
                            .count()
                    })
                    .max()
                    .unwrap_or(0)
            })
            .sum::<usize>();
        if symbol_counts.get(required_symbol).copied().unwrap_or(0)
            + available_from_pool
            + available_from_sources
            < (*required).max(0) as usize
        {
            failed_states.insert(search_state);
            return false;
        }
    }

    let available_snow = mana.iter().filter(|(_, snow)| *snow).count()
        + pool_options
            .iter()
            .enumerate()
            .filter(|(index, (_, _, snow))| !used_pool[*index] && *snow)
            .count()
        + sources
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .map(|(_, (_, choices))| {
                choices
                    .iter()
                    .filter(|choice| choice.life_cost <= remaining_life && choice.snow)
                    .map(|choice| choice.symbols.len())
                    .max()
                    .unwrap_or(0)
            })
            .sum::<usize>();
    if available_snow < requirement.snow.max(0) as usize {
        failed_states.insert(search_state);
        return false;
    }

    let missing_color = missing_colored_symbol(mana, requirement);
    for (pool_option_index, (pool_index, symbol, snow)) in pool_options.iter().enumerate() {
        if used_pool[pool_option_index]
            || missing_color
                .as_ref()
                .is_some_and(|missing| missing != symbol)
        {
            continue;
        }

        used_pool[pool_option_index] = true;
        selected_pool.push(*pool_index);
        mana.push((symbol.clone(), *snow));
        if search_payment(
            pool_options,
            sources,
            requirement,
            used_pool,
            used,
            selected_pool,
            selected,
            mana,
            remaining_life,
            phyrexian_life_paid,
            failed_states,
        ) {
            return true;
        }
        mana.pop();
        selected_pool.pop();
        used_pool[pool_option_index] = false;
    }

    for (source_index, (source_id, choices)) in sources.iter().enumerate() {
        if used[source_index] {
            continue;
        }
        for choice in choices {
            if choice.life_cost > remaining_life {
                continue;
            }
            if missing_color
                .as_ref()
                .is_some_and(|symbol| !choice.symbols.contains(symbol))
            {
                continue;
            }

            used[source_index] = true;
            let symbol_count = mana.len();
            mana.extend(
                choice
                    .symbols
                    .iter()
                    .cloned()
                    .map(|symbol| (symbol, choice.snow)),
            );
            selected.push(ManaActivation {
                source_id: source_id.clone(),
                origin: choice.origin.clone(),
                symbols: choice.symbols.clone(),
                life_cost: choice.life_cost,
                activation_requirement: choice.activation_requirement.clone(),
                sacrifices_source: choice.sacrifices_source,
                declaration: None,
            });
            let mut branch_requirement = requirement.clone();
            branch_requirement.add(&choice.activation_requirement);

            if search_payment(
                pool_options,
                sources,
                &branch_requirement,
                used_pool,
                used,
                selected_pool,
                selected,
                mana,
                remaining_life - choice.life_cost,
                phyrexian_life_paid,
                failed_states,
            ) {
                return true;
            }

            selected.pop();
            mana.truncate(symbol_count);
            used[source_index] = false;
        }
    }
    if let Some(life_paid) = available_phyrexian_payment {
        *phyrexian_life_paid = life_paid;
        return true;
    }
    failed_states.insert(search_state);
    false
}

impl GameEngine {
    /// Searches actual activation order. The projection pays each activation's
    /// declared costs and produces its mana before exposing the next state.
    /// Pool indices in the result refer to that final projected pool.
    pub(super) fn try_plan_ordered_mana_requirement<P>(
        requirement: &ManaRequirement,
        project: &mut P,
    ) -> Result<Option<ManaPayment>, EngineError>
    where
        P: FnMut(&[ManaActivation]) -> Result<OrderedManaState, EngineError>,
    {
        if requirement.is_empty() {
            return Ok(Some(ManaPayment {
                ordered: true,
                ..ManaPayment::default()
            }));
        }
        fn visit<P>(
            requirement: &ManaRequirement,
            project: &mut P,
            prefix: &mut Vec<ManaActivation>,
            projections: &mut usize,
        ) -> Result<Option<ManaPayment>, EngineError>
        where
            P: FnMut(&[ManaActivation]) -> Result<OrderedManaState, EngineError>,
        {
            // Exhaustion is an explicit unsupported search, never a proof that
            // no legal payment exists. The pure production path has no such DFS.
            const MAX_PROJECTIONS: usize = 4096;
            const MAX_ACTIVATIONS: usize = 128;
            if *projections >= MAX_PROJECTIONS {
                return Err(EngineError::new(
                    "ordered mana payment search exceeded its projection budget",
                ));
            }
            *projections += 1;
            let state = project(prefix)?;
            let distinct_indices = state
                .available_mana
                .iter()
                .map(|(index, _, _)| *index)
                .collect::<BTreeSet<_>>();
            if distinct_indices.len() != state.available_mana.len() {
                return Err(EngineError::new(
                    "ordered mana projection contains a duplicate physical pool index",
                ));
            }

            // Reuse the existing allocator with no hypothetical sources. It
            // handles hybrid, snow and Phyrexian requirements from the actual
            // final pool and remaining life. Activation costs are already paid.
            let mut used_pool = vec![false; state.available_mana.len()];
            let mut used_sources = Vec::new();
            let mut selected_pool = Vec::new();
            let mut selected_sources = Vec::new();
            let mut mana = Vec::new();
            let mut phyrexian_life_paid = 0;
            let mut failed_states = BTreeSet::new();
            if search_payment(
                &state.available_mana,
                &[],
                requirement,
                &mut used_pool,
                &mut used_sources,
                &mut selected_pool,
                &mut selected_sources,
                &mut mana,
                state.available_life,
                &mut phyrexian_life_paid,
                &mut failed_states,
            ) {
                let pool_symbols = selected_pool
                    .iter()
                    .map(|index| {
                        state
                            .available_mana
                            .iter()
                            .find(|(pool_index, _, _)| pool_index == index)
                            .expect("allocator selected an index from its pool options")
                            .1
                            .clone()
                    })
                    .collect();
                let cant_be_countered = selected_pool.iter().any(|index| {
                    state
                        .available_mana_cant_be_countered_indices
                        .contains(index)
                });
                return Ok(Some(ManaPayment {
                    activations: prefix.clone(),
                    phyrexian_life_paid,
                    pool_indices: selected_pool,
                    pool_symbols,
                    cant_be_countered,
                    ordered: true,
                }));
            }

            let mut unresolved_branch = state.deferred_branch_error;
            if state.choices.is_empty() {
                return match unresolved_branch {
                    Some(error) => Err(error),
                    None => Ok(None),
                };
            }
            if prefix.len() >= MAX_ACTIVATIONS {
                return Err(EngineError::new(
                    "ordered mana payment search exceeded its activation depth",
                ));
            }
            for option in state.choices {
                prefix.push(ManaActivation {
                    source_id: option.source_id,
                    origin: option.choice.origin.clone(),
                    symbols: option.choice.symbols,
                    life_cost: option.choice.life_cost,
                    activation_requirement: option.choice.activation_requirement,
                    sacrifices_source: option.choice.sacrifices_source,
                    declaration: Some(Box::new(option.declaration)),
                });
                let outcome = visit(requirement, project, prefix, projections);
                prefix.pop();
                match outcome {
                    Ok(Some(payment)) => return Ok(Some(payment)),
                    Ok(None) => {}
                    Err(error) => {
                        // Other complete branches can still prove a payment.
                        // An unexamined/unsupported branch precludes Ok(None).
                        if unresolved_branch.is_none() {
                            unresolved_branch = Some(error);
                        }
                        if *projections >= MAX_PROJECTIONS {
                            break;
                        }
                    }
                }
            }
            match unresolved_branch {
                Some(error) => Err(error),
                None => Ok(None),
            }
        }

        visit(requirement, project, &mut Vec::new(), &mut 0)
    }
}

/// The mana a cast record distinguishes (CR 106.1, 601.2h): the five colors
/// and colorless mana.
const SPENT_MANA_SYMBOLS: [&str; 6] = ["W", "U", "B", "R", "G", "C"];

/// The record a permanent keeps of the mana spent to cast it (CR 601.2h):
/// threshold flags `manaSpent:SYMBOL:N` and `manaSpent:total:N`, and the
/// marker `manaSpentRecorded` that distinguishes zero from an absent record.
pub(super) fn record_mana_spent_flags(
    flags: &mut BTreeMap<String, bool>,
    decisions: &BTreeMap<String, Value>,
) {
    for symbol in SPENT_MANA_SYMBOLS {
        let count = mana_symbol_spent_count(decisions, symbol);
        for threshold in 1..=count {
            flags.insert(format!("manaSpent:{symbol}:{threshold}"), true);
        }
    }
    let total = usize::try_from(mana_spent_from_cast_decisions(decisions)).unwrap_or(0);
    for threshold in 1..=total {
        flags.insert(format!("manaSpent:total:{threshold}"), true);
    }
    flags.insert("manaSpentRecorded".to_string(), true);
}

/// `manaSpentToCast { spell, symbol? }`: the mana of one symbol, or all the
/// mana, spent to cast a spell. `spell` is the ability's own source or the
/// spell a cast trigger observed.
pub(super) fn mana_spent_to_cast_supported(expression: &Value) -> bool {
    matches!(
        value_kind(&expression["spell"]),
        Some("self" | "triggeringSpell")
    ) && expression.get("symbol").is_none_or(|symbol| {
        symbol
            .as_str()
            .is_some_and(|symbol| SPENT_MANA_SYMBOLS.contains(&symbol))
    })
}

/// The amount a cast record answers: one symbol's count, or the total.
fn counted_from_cast_decisions(decisions: &BTreeMap<String, Value>, symbol: Option<&str>) -> i32 {
    match symbol {
        Some(symbol) => {
            i32::try_from(mana_symbol_spent_count(decisions, symbol)).unwrap_or(i32::MAX)
        }
        None => mana_spent_from_cast_decisions(decisions),
    }
}

/// The amount a permanent's record answers. A permanent that was not cast
/// had no mana spent to cast it; a cast permanent without a record is
/// unknown (`None`), never zero.
fn counted_from_permanent_record(card: &CardInstance, symbol: Option<&str>) -> Option<i32> {
    if !card.flags.get("wasCast").copied().unwrap_or(false) {
        return Some(0);
    }
    if !card
        .flags
        .get("manaSpentRecorded")
        .copied()
        .unwrap_or(false)
    {
        return None;
    }
    let key = symbol.unwrap_or("total");
    let mut count = 0_i32;
    while card
        .flags
        .get(&format!("manaSpent:{key}:{}", count + 1))
        .copied()
        .unwrap_or(false)
    {
        count += 1;
    }
    Some(count)
}

/// The per-symbol record of the spell a cast trigger observed, written in
/// the trigger's decisions when the spell became cast.
pub(super) fn triggering_spell_mana_spent_record(
    cast_decisions: &BTreeMap<String, Value>,
) -> Value {
    let mut record = serde_json::Map::new();
    for symbol in SPENT_MANA_SYMBOLS {
        record.insert(
            symbol.to_string(),
            Value::from(counted_from_cast_decisions(cast_decisions, Some(symbol))),
        );
    }
    record.insert(
        "total".to_string(),
        Value::from(mana_spent_from_cast_decisions(cast_decisions)),
    );
    Value::Object(record)
}

impl GameEngine {
    /// The source's own record: its cast decisions while it is the spell
    /// being resolved, else the record of its battlefield incarnation.
    fn source_mana_spent(
        &self,
        source: &CardInstance,
        cast_decisions: Option<&BTreeMap<String, Value>>,
        symbol: Option<&str>,
    ) -> Option<i32> {
        if let Some(decisions) = cast_decisions {
            return Some(counted_from_cast_decisions(decisions, symbol));
        }
        let card = self
            .permanent_position(&source.instance_id)
            .map(|(player, index)| &self.state.players[player].battlefield[index])
            .unwrap_or(source);
        counted_from_permanent_record(card, symbol)
    }

    /// `manaSpentToCast` evaluated with a source and decisions (intervening
    /// `if`, static abilities): the source's permanent record, or the
    /// observed spell's record in the trigger's decisions.
    pub(super) fn mana_spent_to_cast(
        &self,
        expression: &Value,
        source: Option<&CardInstance>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<i32> {
        let symbol = expression["symbol"].as_str();
        match value_kind(&expression["spell"]) {
            Some("self") => self.source_mana_spent(source?, None, symbol),
            Some("triggeringSpell") => triggering_spell_mana_spent(decisions, symbol),
            _ => None,
        }
    }

    /// `manaSpentToCast` during resolution: a resolving spell reads its own
    /// cast decisions; an ability reads its source's permanent record.
    pub(super) fn runtime_mana_spent_to_cast(
        &self,
        expression: &Value,
        stack_object: &StackObject,
    ) -> Option<i32> {
        let symbol = expression["symbol"].as_str();
        match value_kind(&expression["spell"]) {
            Some("self") => self.source_mana_spent(
                &stack_object.card,
                stack_object_is_spell(stack_object).then_some(&stack_object.decisions),
                symbol,
            ),
            Some("triggeringSpell") => triggering_spell_mana_spent(&stack_object.decisions, symbol),
            _ => None,
        }
    }
}

fn triggering_spell_mana_spent(
    decisions: &BTreeMap<String, Value>,
    symbol: Option<&str>,
) -> Option<i32> {
    decisions
        .get("triggeringSpellManaSpentBySymbol")?
        .get(symbol.unwrap_or("total"))?
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_payment_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_payment_ordered_tests.rs"]
mod ordered_tests;
