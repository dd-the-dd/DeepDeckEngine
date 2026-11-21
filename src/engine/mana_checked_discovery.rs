//! Checked production declarations and useful payment sources are distinct.
//! A successful no-output activation remains a declaration under CR605.
use super::mana_production::ManaProductionMode;
use super::mana_projection_context::{ExecutableManaOutputs, ManaProjectionContext};
use super::mana_projection_contract::certify_pure_mana_program;
use super::*;

#[derive(Debug)]
pub(super) enum CheckedManaActivationChoice {
    Produces(ManaChoice),
    NoMana(ManaChoice),
}

#[derive(Debug)]
pub(super) struct CheckedManaActivationPlan {
    pub choices: Vec<CheckedManaActivationChoice>,
}

impl CheckedManaActivationPlan {
    /// Use for explicit declarations, including a genuine zero-output ability.
    pub(super) fn into_activation_choices(self) -> Vec<ManaChoice> {
        self.choices
            .into_iter()
            .map(|choice| match choice {
                CheckedManaActivationChoice::Produces(choice)
                | CheckedManaActivationChoice::NoMana(choice) => choice,
            })
            .collect()
    }

    /// Zero output cannot help pay a mana cost. This is successful semantic
    /// filtering, never conversion of an unsupported projection into empty.
    pub(super) fn into_payment_sources(self) -> Vec<ManaChoice> {
        self.choices
            .into_iter()
            .filter_map(|choice| match choice {
                CheckedManaActivationChoice::Produces(choice) => Some(choice),
                CheckedManaActivationChoice::NoMana(_) => None,
            })
            .collect()
    }
}

impl GameEngine {
    pub(super) fn try_checked_mana_activation_plan(
        &self,
        source: &CardInstance,
        spending_card: Option<&CardDefinition>,
        allow_unassigned_restriction: bool,
        include_nontrivial_costs: bool,
    ) -> Result<CheckedManaActivationPlan, EngineError> {
        let context = ManaProjectionContext::for_value_interpretation(self);
        self.checked_mana_activation_plan_in_context(
            source,
            spending_card,
            allow_unassigned_restriction,
            include_nontrivial_costs,
            &context,
        )
    }

    pub(super) fn checked_mana_activation_plan_in_context(
        &self,
        source: &CardInstance,
        spending_card: Option<&CardDefinition>,
        allow_unassigned_restriction: bool,
        include_nontrivial_costs: bool,
        context: &ManaProjectionContext<'_>,
    ) -> Result<CheckedManaActivationPlan, EngineError> {
        let fail = |state: &str, detail: &str| {
            EngineError::new(format!(
                "unsupported checked mana discovery: source={} state={state} {detail}",
                source.instance_id
            ))
        };
        self.player_index(&source.controller)?;
        if source.flags.get("phasedOut").copied().unwrap_or(false) {
            return Ok(CheckedManaActivationPlan {
                choices: Vec::new(),
            });
        }
        let programs = self.admitted_mana_programs(source)?;
        // These legacy helpers do not yet preserve the target permanent's
        // ability-controller/source context. Cap instead of guessing it.
        if self.has_granted_colorless_mana_ability(source) {
            return Err(fail(
                "mana.discovery.named-grant",
                "typed grant program required",
            ));
        }
        for (granting, modifier) in self.battlefield_static_modifiers() {
            if value_kind(&modifier) == Some("grantManaAbility")
                && Self::static_selector_matches(&granting, &modifier["objects"], source)
                && modifier.get("condition").is_none_or(|condition| {
                    self.condition_value_with_source(
                        condition,
                        &granting.controller,
                        &BTreeMap::new(),
                        Some(&granting),
                    )
                })
            {
                return Err(fail(
                    "mana.discovery.granted-context",
                    "typed recipient/source context required",
                ));
            }
        }
        if self
            .state
            .players
            .iter()
            .flat_map(|p| &p.battlefield)
            .any(|attachment| {
                attachment.attached_to.as_deref() == Some(source.instance_id.as_str())
                    && attachment.definition.rules.iter().any(|rule| {
                        triggered_rule_supported(rule)
                            && value_kind(&rule["event"])
                                == Some("attachedPermanentManaAbilityActivated")
                            && self.rule_active_for_source(attachment, rule)
                    })
            })
        {
            return Err(fail(
                "mana.discovery.attached-program",
                "typed trigger timing/recipient context required",
            ));
        }
        if self.permanent_loses_all_abilities(source) {
            if self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("emrakulExigentLandMana")
                    && modifier["landInstanceId"].as_str() == Some(source.instance_id.as_str())
            }) {
                return Err(fail(
                    "mana.discovery.ability-layer-order",
                    "temporary grant under ability loss requires ordered proof",
                ));
            }
            return Ok(CheckedManaActivationPlan {
                choices: Vec::new(),
            });
        }
        if !self.mana_abilities_replaced_by_land_subtype(source) {
            for program in &programs {
                let rule = &program.rule;
                if value_kind(rule) != Some("manaAbility")
                    || !self.rule_active_for_source(source, rule)
                {
                    continue;
                }
                if !include_nontrivial_costs
                    && !mana_ability_execution::automatic_payment_capability(rule).is_safe()
                {
                    continue;
                }
                if include_nontrivial_costs {
                    // Explicit resolution executes the retained followup program.
                    // Predict only its first direct mana instruction; unlike the
                    // pure counterfactual/automatic path, later nested actions
                    // need not themselves be production-free.
                    let effects = rule["effects"].as_array().ok_or_else(|| {
                        fail("mana.discovery.program", "missing mana effect program")
                    })?;
                    if !mana_rule_supported(rule)
                        || effects.first().and_then(value_kind) != Some("addMana")
                        || rule.get("condition").is_some()
                        || effects
                            .iter()
                            .filter(|effect| value_kind(effect) == Some("addMana"))
                            .count()
                            != 1
                    {
                        return Err(fail(
                            "mana.discovery.primary-program",
                            "one first direct mana instruction followed by executable nested actions",
                        ));
                    }
                } else {
                    if let Some(index) = program.origin.native_index() {
                        certify_pure_mana_program(&source.instance_id, index, rule).map_err(
                            |error| {
                                fail(
                                    "mana.discovery.program",
                                    &format!("rule={} state={:?}", error.rule_index, error.state),
                                )
                            },
                        )?;
                    }
                }
                let actor = &rule["effects"][0]["player"];
                if value_kind(actor) != Some("abilityController")
                    && !(value_kind(actor) == Some("controllerOf")
                        && matches!(value_kind(&actor["object"]), Some("self" | "abilitySource")))
                {
                    return Err(fail(
                        "mana.discovery.recipient",
                        "mana is not credited to the source controller",
                    ));
                }
                // The certificate is a shape boundary, not a value fallback.
                // Resolution quantities remain owned by the shared typed walker.
            }
        }
        let mut resolve = |value: &Value,
                           controller: &str,
                           _visited: &BTreeSet<String>,
                           actual_source: Option<&CardInstance>| {
            let actual_source = actual_source
                .ok_or_else(|| fail("mana.discovery.source-context", "missing value source"))?;
            match context
                .executable_outputs(value, controller, actual_source)
                .map_err(|failure| failure.into_engine_error())?
            {
                ExecutableManaOutputs::NoMana(_) => Ok(vec![Vec::new()]),
                ExecutableManaOutputs::Choices(choices) if choices.is_empty() => Err(fail(
                    "mana.discovery.empty-choice-invariant",
                    "typed output alternatives unexpectedly empty",
                )),
                ExecutableManaOutputs::Choices(choices) => Ok(choices),
            }
        };
        let choices = self.try_plan_admitted_mana_production(
            source,
            &programs,
            spending_card,
            allow_unassigned_restriction,
            include_nontrivial_costs,
            &BTreeSet::from([source.instance_id.clone()]),
            ManaProductionMode::CheckedActivation,
            &mut resolve,
        )?;
        Ok(CheckedManaActivationPlan {
            choices: choices
                .into_iter()
                .map(|choice| {
                    if choice.symbols.is_empty() {
                        CheckedManaActivationChoice::NoMana(choice)
                    } else {
                        CheckedManaActivationChoice::Produces(choice)
                    }
                })
                .collect(),
        })
    }

    pub(super) fn try_checked_payment_for_mana_costs_with_context(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
    ) -> Result<Option<ManaPayment>, EngineError> {
        let context = ManaProjectionContext::for_value_interpretation(self);
        self.checked_payment_in_context(
            player_index,
            mana_costs,
            excluded_sources,
            spending_card,
            ability_source,
            spending_source_zone,
            &context,
        )
    }

    pub(super) fn checked_payment_in_context(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        context: &ManaProjectionContext<'_>,
    ) -> Result<Option<ManaPayment>, EngineError> {
        if self.requires_ordered_mana_payment(player_index) {
            let mut requirement = ManaRequirement::default();
            for cost in mana_costs {
                let Some(parsed) = parse_mana_requirement(cost) else {
                    return Ok(None);
                };
                requirement.add(&parsed);
            }
            return Self::try_plan_ordered_mana_requirement(&requirement, &mut |prefix| {
                self.project_ordered_mana_prefix(
                    player_index,
                    prefix,
                    excluded_sources,
                    spending_card,
                    ability_source,
                    spending_source_zone,
                )
            });
        }
        self.try_plan_mana_payment(
            player_index,
            mana_costs,
            excluded_sources,
            spending_card,
            ability_source,
            spending_source_zone,
            &mut |source, spending| {
                self.checked_mana_activation_plan_in_context(
                    source, spending, false, false, context,
                )
                .map(CheckedManaActivationPlan::into_payment_sources)
            },
        )
    }
    pub(super) fn checked_payment_requirement_in_context(
        &self,
        player_index: usize,
        requirement: ManaRequirement,
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        context: &ManaProjectionContext<'_>,
    ) -> Result<Option<ManaPayment>, EngineError> {
        if self.requires_ordered_mana_payment(player_index) {
            return Self::try_plan_ordered_mana_requirement(&requirement, &mut |prefix| {
                self.project_ordered_mana_prefix(
                    player_index,
                    prefix,
                    excluded_sources,
                    spending_card,
                    ability_source,
                    spending_source_zone,
                )
            });
        }
        self.try_plan_mana_requirement(
            player_index,
            requirement,
            excluded_sources,
            spending_card,
            ability_source,
            spending_source_zone,
            &mut |source, spending| {
                self.checked_mana_activation_plan_in_context(
                    source, spending, false, false, context,
                )
                .map(CheckedManaActivationPlan::into_payment_sources)
            },
            Instant::now(),
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_checked_discovery_tests.rs"]
mod tests;
