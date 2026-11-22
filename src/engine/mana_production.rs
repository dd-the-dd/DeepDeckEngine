//! Shared mana production and hypothetical-type discovery.
//! Shared planner accepts a fallible value resolver. The historical activation
//! adapter is genuinely Infallible; the counterfactual evaluator is not yet wired.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManaProductionMode {
    Activation,
    CheckedActivation,
    Counterfactual,
}

impl GameEngine {
    pub(super) fn mana_abilities_replaced_by_land_subtype(&self, permanent: &CardInstance) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                (value_kind(modifier) == Some("nonbasicLandsBecomeMountains")
                    && is_land(&permanent.definition)
                    && !permanent.definition.type_line.starts_with("Basic ")
                    && self.condition_value_with_source(
                        &modifier["condition"],
                        &source.controller,
                        &BTreeMap::new(),
                        Some(source),
                    ))
                    || (value_kind(modifier) == Some("setLandSubtype")
                        && Self::static_selector_matches(source, &modifier["objects"], permanent))
            })
    }

    fn legacy_mana_resolver(
        &self,
    ) -> impl FnMut(
        &Value,
        &str,
        &BTreeSet<String>,
        Option<&CardInstance>,
    ) -> Result<Vec<Vec<String>>, std::convert::Infallible>
    + '_ {
        |value, controller_id, visited_sources, source| {
            Ok(self.mana_options_for_value(value, controller_id, visited_sources, source))
        }
    }

    pub(super) fn granted_mana_choices(&self, permanent: &CardInstance) -> Vec<ManaChoice> {
        let visited_sources = BTreeSet::from([permanent.instance_id.clone()]);
        match self.try_granted_mana_choices(
            permanent,
            &visited_sources,
            &mut self.legacy_mana_resolver(),
        ) {
            Ok(choices) => choices,
            Err(never) => match never {},
        }
    }

    fn try_granted_mana_choices<E, F>(
        &self,
        permanent: &CardInstance,
        visited_sources: &BTreeSet<String>,
        resolve: &mut F,
    ) -> Result<Vec<ManaChoice>, E>
    where
        F: FnMut(
            &Value,
            &str,
            &BTreeSet<String>,
            Option<&CardInstance>,
        ) -> Result<Vec<Vec<String>>, E>,
    {
        let mut choices = Vec::new();
        for (source, modifier) in
            self.battlefield_static_modifiers()
                .into_iter()
                .filter(|(source, modifier)| {
                    value_kind(modifier) == Some("grantManaAbility")
                        && Self::static_selector_matches(source, &modifier["objects"], permanent)
                        && modifier.get("condition").is_none_or(|condition| {
                            self.condition_value_with_source(
                                condition,
                                &source.controller,
                                &BTreeMap::new(),
                                Some(source),
                            )
                        })
                })
        {
            choices.extend(
                resolve(
                    &modifier["mana"],
                    &source.controller,
                    visited_sources,
                    Some(&source),
                )?
                .into_iter()
                .map(|symbols| ManaChoice {
                    intrinsic_symbols: Some(symbols.clone()),
                    origin: None,
                    symbols,
                    snow: false,
                    life_cost: 0,
                    spend_restriction: modifier.get("spendRestriction").cloned(),
                    activation_requirement: ManaRequirement::default(),
                    sacrifices_source: false,
                }),
            );
        }
        choices.extend(
            self.state
                .rule_modifiers
                .iter()
                .filter(|modifier| {
                    value_kind(modifier) == Some("emrakulExigentLandMana")
                        && modifier["landInstanceId"].as_str()
                            == Some(permanent.instance_id.as_str())
                        && modifier["sourceCardInstanceId"]
                            .as_str()
                            .is_some_and(|source_id| {
                                !self.state.events.iter().any(|event| {
                                    event.kind == "spellCast"
                                        && event.card_instance_id.as_deref() == Some(source_id)
                                        && event.detail["fromZone"].as_str() == Some("exile")
                                })
                            })
                })
                .map(|_| ManaChoice {
                    intrinsic_symbols: Some(vec!["C".to_string(), "C".to_string()]),
                    origin: None,
                    symbols: vec!["C".to_string(), "C".to_string()],
                    snow: false,
                    life_cost: 0,
                    spend_restriction: None,
                    activation_requirement: ManaRequirement::default(),
                    sacrifices_source: false,
                }),
        );
        Ok(choices)
    }
    pub(super) fn attached_mana_trigger_options(
        &self,
        permanent: &CardInstance,
    ) -> Vec<Vec<String>> {
        match self.try_attached_mana_trigger_options(permanent, &mut self.legacy_mana_resolver()) {
            Ok(options) => options,
            Err(never) => match never {},
        }
    }

    fn try_attached_mana_trigger_options<E, F>(
        &self,
        permanent: &CardInstance,
        resolve: &mut F,
    ) -> Result<Vec<Vec<String>>, E>
    where
        F: FnMut(
            &Value,
            &str,
            &BTreeSet<String>,
            Option<&CardInstance>,
        ) -> Result<Vec<Vec<String>>, E>,
    {
        let attachments = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|attachment| {
                attachment.attached_to.as_deref() == Some(permanent.instance_id.as_str())
            });
        let mut combined = vec![Vec::new()];
        let mut found_trigger = false;
        for attachment in attachments {
            for rule in attachment.definition.rules.iter().filter(|rule| {
                triggered_rule_supported(rule)
                    && value_kind(&rule["event"]) == Some("attachedPermanentManaAbilityActivated")
                    && self.rule_active_for_source(attachment, rule)
            }) {
                for effect in rule["effects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|effect| value_kind(effect) == Some("addMana"))
                {
                    let options = if value_kind(&effect["mana"]) == Some("storedColor") {
                        let decision_id = effect["mana"]["decisionId"]
                            .as_str()
                            .unwrap_or("chosenColor");
                        let amount = integer_value(&effect["mana"]["amount"])
                            .and_then(|amount| usize::try_from(amount).ok())
                            .unwrap_or(1);
                        stored_decision_value(&attachment.definition, decision_id)
                            .map(|symbol| vec![vec![symbol.to_string(); amount]])
                            .unwrap_or_default()
                    } else {
                        resolve(
                            &effect["mana"],
                            &permanent.controller,
                            &BTreeSet::from([permanent.instance_id.clone()]),
                            Some(attachment),
                        )?
                    };
                    if options.is_empty() {
                        continue;
                    }
                    found_trigger = true;
                    combined = combined
                        .into_iter()
                        .flat_map(|base| {
                            options.iter().map(move |extra| {
                                let mut symbols = base.clone();
                                symbols.extend(extra.clone());
                                symbols
                            })
                        })
                        .collect();
                }
            }
        }
        Ok(if found_trigger { combined } else { Vec::new() })
    }
    pub(super) fn mana_choices_for_permanent(
        &self,
        permanent: &CardInstance,
        spending_card: Option<&CardDefinition>,
        allow_unassigned_restriction: bool,
        include_nontrivial_costs: bool,
    ) -> Vec<ManaChoice> {
        let visited_sources = BTreeSet::from([permanent.instance_id.clone()]);
        match self.try_plan_mana_production(
            permanent,
            spending_card,
            allow_unassigned_restriction,
            include_nontrivial_costs,
            &visited_sources,
            ManaProductionMode::Activation,
            &mut self.legacy_mana_resolver(),
        ) {
            Ok(choices) => choices,
            Err(never) => match never {},
        }
    }

    /// Low-level planner: Counterfactual callers must separately certify all
    /// producer program shapes and value bindings. This method alone is not a
    /// complete could-produce capability certificate.
    pub(super) fn try_plan_mana_production<E, F>(
        &self,
        permanent: &CardInstance,
        spending_card: Option<&CardDefinition>,
        allow_unassigned_restriction: bool,
        include_nontrivial_costs: bool,
        visited_sources: &BTreeSet<String>,
        mode: ManaProductionMode,
        resolve: &mut F,
    ) -> Result<Vec<ManaChoice>, E>
    where
        F: FnMut(
            &Value,
            &str,
            &BTreeSet<String>,
            Option<&CardInstance>,
        ) -> Result<Vec<Vec<String>>, E>,
    {
        self.try_plan_admitted_mana_production(
            permanent,
            &granted_mana_program::native_mana_programs(permanent),
            spending_card,
            allow_unassigned_restriction,
            include_nontrivial_costs,
            visited_sources,
            mode,
            resolve,
        )
    }

    pub(super) fn try_plan_admitted_mana_production<E, F>(
        &self,
        permanent: &CardInstance,
        programs: &[granted_mana_program::AdmittedManaProgram],
        spending_card: Option<&CardDefinition>,
        allow_unassigned_restriction: bool,
        include_nontrivial_costs: bool,
        visited_sources: &BTreeSet<String>,
        mode: ManaProductionMode,
        resolve: &mut F,
    ) -> Result<Vec<ManaChoice>, E>
    where
        F: FnMut(
            &Value,
            &str,
            &BTreeSet<String>,
            Option<&CardInstance>,
        ) -> Result<Vec<Vec<String>>, E>,
    {
        let counterfactual = mode == ManaProductionMode::Counterfactual;
        if counterfactual && permanent.flags.get("phasedOut").copied().unwrap_or(false) {
            return Ok(Vec::new());
        }
        let mut choices = Vec::new();
        let counterfactual_printed_abilities_removed =
            counterfactual && self.mana_abilities_replaced_by_land_subtype(permanent);
        // A mana ability functions only from its declared activation zone,
        // the battlefield by default. A hypothetical source outside every zone
        // is planned as a battlefield producer.
        let source_zone = if counterfactual {
            "battlefield"
        } else {
            self.object_zone_name(&permanent.instance_id)
                .unwrap_or("battlefield")
        };
        for candidate in programs {
            let rule = &candidate.rule;
            if counterfactual_printed_abilities_removed
                && matches!(candidate.origin, ManaProgramOrigin::Native { .. })
            {
                // Removed printed abilities are not hypothetical producers.
                // Do not resolve their values before subsequently clearing them.
                continue;
            }
            if !mana_rule_supported(rule)
                || !activated_rule_usable_from_zone(rule, source_zone)
                || !self.rule_active_for_source(permanent, rule)
            {
                continue;
            }
            if !counterfactual
                && !include_nontrivial_costs
                && !mana_ability_execution::automatic_payment_capability(rule).is_safe()
            {
                continue;
            }
            if !counterfactual
                && rule.get("activationLimit").is_some_and(|limit| {
                    let Some(flag) = granted_mana_program::mana_activation_limit_flag(
                        limit,
                        self.state.turn_number,
                        Some(&candidate.origin),
                    ) else {
                        return false;
                    };
                    permanent.flags.get(&flag) == Some(&true)
                })
            {
                continue;
            }
            if !counterfactual
                && !include_nontrivial_costs
                && rule["costs"].as_array().is_some_and(|costs| {
                    costs.iter().any(|cost| {
                        !matches!(value_kind(cost), Some("tap" | "payLife" | "payMana"))
                            && !(value_kind(cost) == Some("sacrificePermanent")
                                && value_kind(&cost["permanent"]) == Some("self"))
                    })
                })
            {
                continue;
            }
            if !counterfactual
                && let Some(condition) = rule.get("activationCondition")
                && !self.condition_value_with_source(
                    condition,
                    &permanent.controller,
                    &BTreeMap::new(),
                    Some(permanent),
                )
            {
                continue;
            }
            let life_cost = if counterfactual {
                0
            } else {
                rule["costs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|cost| value_kind(cost) == Some("payLife"))
                    .filter_map(|cost| integer_value(&cost["amount"]))
                    .sum::<i32>()
            };
            let can_pay_life = self
                .state
                .players
                .iter()
                .find(|player| player.id == permanent.controller)
                .is_some_and(|player| player.life >= life_cost);
            if !counterfactual && !can_pay_life {
                continue;
            }
            let can_remove_counters = counterfactual
                || rule["costs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|cost| value_kind(cost) == Some("removeCounters"))
                    .all(|cost| Self::remove_counter_cost_has_option(permanent, cost));
            if !counterfactual && !can_remove_counters {
                continue;
            }
            let activation_requirement = if counterfactual {
                // Hypothetical resolution does not require a payment plan.
                // In particular an unresolved X in a cost must not erase an
                // independently fixed output type.
                Some(ManaRequirement::default())
            } else {
                rule["costs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|cost| value_kind(cost) == Some("payMana"))
                    .try_fold(ManaRequirement::default(), |mut requirement, cost| {
                        let parsed = parse_mana_requirement(cost["manaCost"].as_str()?)?;
                        requirement.add(&parsed);
                        Some(requirement)
                    })
            };
            let Some(activation_requirement) = activation_requirement else {
                continue;
            };
            let sacrifices_source = rule["costs"].as_array().is_some_and(|costs| {
                costs.iter().any(|cost| {
                    value_kind(cost) == Some("sacrificePermanent")
                        && value_kind(&cost["permanent"]) == Some("self")
                })
            });
            for effect in rule["effects"].as_array().into_iter().flatten() {
                if value_kind(effect) == Some("addMana") {
                    let mut spend_restriction = effect
                        .get("spendRestriction")
                        .cloned()
                        .or_else(|| effect["mana"].get("spendRestriction").cloned());
                    if let Some(restriction) = &mut spend_restriction
                        && restriction["chosenCreatureTypeFromSource"].as_bool() == Some(true)
                    {
                        let Some(chosen_type) =
                            stored_decision_value(&permanent.definition, "chosenCreatureType")
                        else {
                            continue;
                        };
                        restriction["where"] = json!({
                            "kind": "and",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Creature" },
                                { "kind": "subtypeContains", "value": chosen_type },
                            ],
                        });
                    }
                    if let Some(restriction) = &spend_restriction
                        && !allow_unassigned_restriction
                    {
                        let Some(spending_card) = spending_card else {
                            continue;
                        };
                        if !matches_card_filter(spending_card, &restriction["where"]) {
                            continue;
                        }
                    }
                    let mut mana = effect["mana"].clone();
                    if mode != ManaProductionMode::CheckedActivation
                        && value_kind(&mana["amount"]) == Some("countChosenCreatureType")
                    {
                        let decision_id = mana["amount"]["decisionId"]
                            .as_str()
                            .unwrap_or("chosenCreatureType");
                        let amount = stored_decision_value(&permanent.definition, decision_id)
                            .map(|chosen_type| {
                                self.state.players
                                    [self.player_index(&permanent.controller).unwrap_or(0)]
                                .battlefield
                                .iter()
                                .filter(|candidate| {
                                    self.permanent_is_creature(candidate)
                                        && (has_keyword(&candidate.definition, "changeling")
                                            || type_line_contains(
                                                &candidate.definition.type_line,
                                                chosen_type,
                                            ))
                                })
                                .count() as i32
                            })
                            .unwrap_or(0);
                        mana["amount"] = Value::from(amount);
                    }
                    choices.extend(
                        resolve(
                            &mana,
                            &permanent.controller,
                            visited_sources,
                            Some(permanent),
                        )?
                        .into_iter()
                        .map(|symbols| ManaChoice {
                            intrinsic_symbols: None,
                            origin: Some(candidate.origin.clone()),
                            symbols,
                            snow: false,
                            life_cost,
                            spend_restriction: spend_restriction.clone(),
                            activation_requirement: activation_requirement.clone(),
                            sacrifices_source,
                        }),
                    );
                }
            }
        }
        let replaced_land_subtypes = self.mana_abilities_replaced_by_land_subtype(permanent);
        if replaced_land_subtypes {
            choices.clear();
        }
        if is_land(&permanent.definition) {
            for symbols in self.static_basic_land_mana(permanent) {
                if !choices.iter().any(|choice| {
                    Self::mana_choice_matches_intrinsic_land_ability(permanent, choice, &symbols)
                }) {
                    choices.push(ManaChoice {
                        intrinsic_symbols: Some(symbols.clone()),
                        origin: None,
                        symbols,
                        snow: false,
                        life_cost: 0,
                        spend_restriction: None,
                        activation_requirement: ManaRequirement::default(),
                        sacrifices_source: false,
                    });
                }
            }
        }
        for choice in self.try_granted_mana_choices(permanent, visited_sources, resolve)? {
            if !choices
                .iter()
                .any(|candidate| candidate.origin.is_none() && candidate.symbols == choice.symbols)
            {
                choices.push(choice);
            }
        }
        if self.has_granted_colorless_mana_ability(permanent)
            && !choices.iter().any(|choice| {
                choice.origin.is_none() && choice.symbols.len() == 1 && choice.symbols[0] == "C"
            })
        {
            choices.push(ManaChoice {
                intrinsic_symbols: Some(vec!["C".to_string()]),
                origin: None,
                symbols: vec!["C".to_string()],
                snow: false,
                life_cost: 0,
                spend_restriction: None,
                activation_requirement: ManaRequirement::default(),
                sacrifices_source: false,
            });
        }
        self.finish_mana_production_choices(
            permanent,
            &permanent.controller,
            choices,
            counterfactual,
            resolve,
        )
    }

    /// Production-event transformations are independent of eligibility and
    /// payment. Both discovery and post-cost resolution use this same service.
    pub(super) fn finish_mana_production_choices<E, F>(
        &self,
        permanent: &CardInstance,
        ability_controller: &str,
        mut choices: Vec<ManaChoice>,
        counterfactual: bool,
        resolve: &mut F,
    ) -> Result<Vec<ManaChoice>, E>
    where
        F: FnMut(
            &Value,
            &str,
            &BTreeSet<String>,
            Option<&CardInstance>,
        ) -> Result<Vec<Vec<String>>, E>,
    {
        // Counterfactual callers project TYPES, not quantities. Their certified
        // context rejects cardinality-dependent type transformations; positive
        // multipliers preserve that type domain and need not be materialized.
        let production_factor = if counterfactual {
            1
        } else {
            self.mana_production_factor(ability_controller)
        };
        if production_factor > 1 {
            for choice in &mut choices {
                let taps_permanent = choice.program(permanent).map_or(true, |rule| {
                    rule["costs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|cost| value_kind(cost) == Some("tap"))
                });
                if taps_permanent {
                    let produced = choice.symbols.clone();
                    choice.symbols = (0..production_factor)
                        .flat_map(|_| produced.clone())
                        .collect();
                }
            }
        }
        let attached_mana_options = if counterfactual {
            Vec::new()
        } else {
            self.try_attached_mana_trigger_options(permanent, resolve)?
        };
        if !attached_mana_options.is_empty() {
            choices = choices
                .into_iter()
                .flat_map(|choice| {
                    attached_mana_options.iter().map(move |extra| {
                        let mut augmented = choice.clone();
                        augmented.symbols.extend(extra.clone());
                        augmented
                    })
                })
                .collect();
        }
        if is_land(&permanent.definition)
            && self
                .battlefield_static_modifiers()
                .iter()
                .any(|(_, modifier)| value_kind(modifier) == Some("dampingSphereMana"))
        {
            for choice in &mut choices {
                if choice.symbols.len() >= 2 {
                    choice.symbols = vec!["C".to_string()];
                }
            }
        }
        let produced_by_snow_source = type_line_contains(&permanent.definition.type_line, "Snow");
        for choice in &mut choices {
            choice.snow = produced_by_snow_source;
        }
        Ok(choices)
    }
    pub(super) fn mana_options_for_value(
        &self,
        value: &Value,
        controller_id: &str,
        visited_sources: &BTreeSet<String>,
        source: Option<&CardInstance>,
    ) -> Vec<Vec<String>> {
        if value_kind(value) == Some("fixedMana") {
            let Some(symbol) = value["symbol"].as_str() else {
                return Vec::new();
            };
            let amount = self
                .numeric_expression_with_source(&value["amount"], controller_id, source)
                .and_then(|amount| usize::try_from(amount.max(0)).ok())
                .unwrap_or(0);
            return (amount > 0)
                .then(|| vec![vec![symbol.to_string(); amount]])
                .unwrap_or_default();
        }
        if value_kind(value) == Some("chooseCommanderColor") {
            let amount = integer_value(&value["amount"])
                .and_then(|amount| usize::try_from(amount).ok())
                .unwrap_or(1);
            let colors = self
                .state
                .players
                .iter()
                .find(|player| player.id == controller_id)
                .into_iter()
                .flat_map(|player| {
                    player
                        .library
                        .iter()
                        .chain(&player.hand)
                        .chain(&player.battlefield)
                        .chain(&player.graveyard)
                        .chain(&player.exile)
                        .chain(&player.sideboard)
                        .chain(&player.command_zone)
                })
                .filter(|card| is_commander_definition(&card.definition))
                .flat_map(|card| card_color_symbols(&card.definition))
                .collect::<BTreeSet<_>>();
            let colors = if colors.is_empty() {
                ["W", "U", "B", "R", "G"]
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect::<BTreeSet<_>>()
            } else {
                colors
            };
            return colors
                .into_iter()
                .map(|symbol| vec![symbol; amount])
                .collect();
        }
        if value_kind(value) == Some("eachColorAmongPermanents") {
            let Ok(player_index) = self.player_index(controller_id) else {
                return Vec::new();
            };
            let colors = ["W", "U", "B", "R", "G"]
                .iter()
                .filter(|symbol| {
                    self.state.players[player_index]
                        .battlefield
                        .iter()
                        .filter(|card| {
                            value.get("where").is_none_or(|where_filter| {
                                self.permanent_state_matches(card, where_filter)
                            })
                        })
                        .any(|card| card_color_symbols(&card.definition).contains(**symbol))
                })
                .map(|symbol| symbol.to_string())
                .collect::<Vec<_>>();
            return (!colors.is_empty()).then_some(colors).into_iter().collect();
        }
        if value_kind(value) == Some("linkedExiledCardColors") {
            let Some(source) = source else {
                return Vec::new();
            };
            let amount = integer_value(&value["amount"])
                .and_then(|amount| usize::try_from(amount.max(0)).ok())
                .unwrap_or(1);
            return self
                .state
                .rule_modifiers
                .iter()
                .filter(|modifier| {
                    value_kind(modifier) == Some("exiledWithSource")
                        && modifier["sourceCardInstanceId"].as_str()
                            == Some(source.instance_id.as_str())
                })
                .filter_map(|modifier| modifier["cardInstanceId"].as_str())
                .filter_map(|instance_id| self.card_outside_battlefield(instance_id))
                .flat_map(|card| card_color_symbols(&card.definition))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|symbol| vec![symbol; amount])
                .collect();
        }
        if value_kind(value) == Some("chooseColor") {
            if value_kind(&value["amount"]) == Some("devotionToChosenColor") {
                let Ok(player_index) = self.player_index(controller_id) else {
                    return Vec::new();
                };
                return ["W", "U", "B", "R", "G"]
                    .into_iter()
                    .map(|symbol| {
                        let amount = self.state.players[player_index]
                            .battlefield
                            .iter()
                            .flat_map(|card| parse_mana_symbols(&card.definition.mana_cost))
                            .filter(|candidate| {
                                candidate.split('/').any(|component| component == symbol)
                            })
                            .count();
                        vec![symbol.to_string(); amount]
                    })
                    .collect();
            }
            let amount = self
                .numeric_expression_with_source(&value["amount"], controller_id, source)
                .or_else(|| integer_value(&value["amount"]))
                .and_then(|amount| usize::try_from(amount.max(0)).ok())
                .unwrap_or(1);
            return ["W", "U", "B", "R", "G"]
                .into_iter()
                .map(|symbol| vec![symbol.to_string(); amount])
                .collect();
        }
        if value_kind(value) == Some("manaColorsLandsOpponentsControlCouldProduce") {
            let amount = integer_value(&value["amount"])
                .and_then(|amount| usize::try_from(amount).ok())
                .unwrap_or(1);
            return self
                .mana_colors_lands_opponents_control_could_produce(controller_id, visited_sources)
                .into_iter()
                .map(|symbol| vec![symbol; amount])
                .collect();
        }
        if value_kind(value) == Some("manaTypesOfSacrificedPermanent") {
            let amount = integer_value(&value["amount"])
                .and_then(|amount| usize::try_from(amount).ok())
                .unwrap_or(1);
            let Some(player_index) = self
                .state
                .players
                .iter()
                .position(|player| player.id == controller_id)
            else {
                return Vec::new();
            };
            let types = self.state.players[player_index]
                .battlefield
                .iter()
                .filter(|permanent| !visited_sources.contains(&permanent.instance_id))
                .flat_map(|permanent| {
                    self.mana_types_permanent_could_produce(permanent, visited_sources)
                })
                .collect::<BTreeSet<_>>();
            return types
                .into_iter()
                .map(|symbol| vec![symbol; amount])
                .collect();
        }
        if value_kind(value) != Some("manaTypesLandsYouControlCouldProduce") {
            return mana_options_from_value(value);
        }
        let amount = integer_value(&value["amount"])
            .and_then(|amount| usize::try_from(amount).ok())
            .unwrap_or(1);
        self.mana_types_lands_you_control_could_produce(controller_id, visited_sources)
            .into_iter()
            .map(|symbol| vec![symbol; amount])
            .collect()
    }
    pub(super) fn mana_types_lands_you_control_could_produce(
        &self,
        controller_id: &str,
        visited_sources: &BTreeSet<String>,
    ) -> Vec<String> {
        let player_ids = BTreeSet::from([controller_id.to_string()]);
        self.mana_types_lands_players_control_could_produce(&player_ids, visited_sources, true)
    }
    pub(super) fn mana_colors_lands_opponents_control_could_produce(
        &self,
        controller_id: &str,
        visited_sources: &BTreeSet<String>,
    ) -> Vec<String> {
        let player_ids = self
            .state
            .players
            .iter()
            .filter(|player| player.id != controller_id)
            .map(|player| player.id.clone())
            .collect::<BTreeSet<_>>();
        self.mana_types_lands_players_control_could_produce(&player_ids, visited_sources, false)
    }
    pub(super) fn mana_types_lands_players_control_could_produce(
        &self,
        player_ids: &BTreeSet<String>,
        visited_sources: &BTreeSet<String>,
        include_colorless: bool,
    ) -> Vec<String> {
        let mut produced_types = BTreeSet::new();
        for land in self
            .state
            .players
            .iter()
            .filter(|player| player_ids.contains(&player.id))
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| is_land(&permanent.definition))
            .filter(|permanent| !visited_sources.contains(&permanent.instance_id))
        {
            produced_types.extend(self.mana_types_permanent_could_produce(land, visited_sources));
        }
        ["W", "U", "B", "R", "G", "C"]
            .iter()
            .filter(|symbol| {
                (include_colorless || **symbol != "C") && produced_types.contains(**symbol)
            })
            .map(|symbol| (*symbol).to_string())
            .collect()
    }
    pub(super) fn mana_types_permanent_could_produce(
        &self,
        permanent: &CardInstance,
        visited_sources: &BTreeSet<String>,
    ) -> Vec<String> {
        if visited_sources.contains(&permanent.instance_id) {
            return Vec::new();
        }
        let mut branch_sources = visited_sources.clone();
        branch_sources.insert(permanent.instance_id.clone());
        let mut produced_types = basic_land_mana(&permanent.definition)
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>();
        for effect in permanent
            .definition
            .rules
            .iter()
            .filter(|rule| value_kind(rule) == Some("manaAbility"))
            .flat_map(|rule| rule["effects"].as_array().into_iter().flatten())
            .filter(|effect| value_kind(effect) == Some("addMana"))
        {
            for option in self.mana_options_for_value(
                &effect["mana"],
                &permanent.controller,
                &branch_sources,
                Some(permanent),
            ) {
                produced_types.extend(option);
            }
        }
        ["W", "U", "B", "R", "G", "C"]
            .iter()
            .filter(|symbol| produced_types.contains(**symbol))
            .map(|symbol| (*symbol).to_string())
            .collect()
    }
    pub(super) fn sacrificed_permanent_mana_symbols_match(
        &self,
        rule: &Value,
        targets: &BTreeMap<String, TargetRef>,
        symbols: &[String],
    ) -> bool {
        let Some(mana_value) = rule["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|effect| value_kind(effect) == Some("addMana"))
            .map(|effect| &effect["mana"])
            .filter(|mana| value_kind(mana) == Some("manaTypesOfSacrificedPermanent"))
        else {
            return true;
        };
        let Some(decision_id) = mana_value["decisionId"].as_str() else {
            return false;
        };
        let Some(TargetRef::Permanent { instance_id }) = targets.get(decision_id) else {
            return false;
        };
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return false;
        };
        let permanent = &self.state.players[player_index].battlefield[card_index];
        let types = self
            .mana_types_permanent_could_produce(permanent, &BTreeSet::new())
            .into_iter()
            .collect::<BTreeSet<_>>();
        !symbols.is_empty() && symbols.iter().all(|symbol| types.contains(symbol))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_production_tests.rs"]
mod tests;
