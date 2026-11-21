//! Executable production belongs to the world after the ability costs.
//! This boundary is separate from CR106.7 hypothetical type witnesses.
use super::mana_projection_context::ExecutableManaOutputs;
use super::*;

/// A structured value can observe world state or activation bindings. Literal
/// production retains the existing fast path unless its costs can change the
/// production-event modifiers. This is a semantic risk predicate, not a grammar.
pub(super) fn requires_postcost_production(rule: &Value) -> bool {
    !rule["effects"][0]["mana"].is_string()
        || rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|cost| !matches!(value_kind(cost), Some("tap")))
}

pub(super) fn source_history_required(rule: &Value) -> bool {
    fn reads_source(node: &Value) -> bool {
        if matches!(
            value_kind(node),
            Some("powerOf" | "toughnessOf" | "countCounters" | "countAttachments")
        ) && matches!(value_kind(&node["object"]), Some("self" | "abilitySource"))
        {
            return true;
        }
        match node {
            Value::Object(values) => values.values().any(reads_source),
            Value::Array(values) => values.iter().any(reads_source),
            _ => false,
        }
    }
    let can_depart = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|cost| value_kind(cost) == Some("sacrificePermanent"));
    can_depart
        && (reads_source(&rule["effects"])
            || mana_ability_execution::followup_effects(rule)
                .next()
                .is_some())
}

pub(super) fn immediate_runtime_source(action: &LegalAction, source: &CardInstance) -> StackObject {
    StackObject {
        id: format!("mana-ability:{}", action.id),
        controller: action.player_id.clone(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("manaAbility".to_string()),
        ability_rule: None,
        decisions: action.decisions.clone(),
        targets: action.targets.clone(),
    }
}

impl GameEngine {
    /// Costs and production modifiers share one risk boundary across discovery,
    /// shadow planning, actual resolution and checkpoint selection. Inspect raw
    /// conditional modifiers BEFORE evaluating their condition, since a paid
    /// tap/life cost can make a previously inactive modifier applicable.
    pub(super) fn mana_production_requires_postcost(&self, rule: Option<&Value>) -> bool {
        rule.is_some_and(requires_postcost_production)
            || self
                .state
                .players
                .iter()
                .flat_map(|p| &p.battlefield)
                .any(|source| {
                    source.definition.rules.iter().any(|rule| {
                        value_kind(rule) == Some("staticAbility")
                            && rule["modifiers"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|modifier| {
                                    matches!(
                                        value_kind(modifier),
                                        Some("multiplyManaProduction" | "dampingSphereMana")
                                    ) && (rule.get("condition").is_some()
                                        || modifier.get("condition").is_some()
                                        || rule.get("activeWhile").is_some_and(|active| {
                                            value_kind(active) != Some("inZone")
                                        }))
                                })
                    })
                })
    }

    /// Eligibility has already been checked. Do not rediscover an ability from
    /// its output value or demand that its paid costs remain payable afterward.
    pub(super) fn resolve_postcost_mana_choices(
        &self,
        rule: Option<&Value>,
        declared: &ManaChoice,
        runtime_source: &StackObject,
    ) -> Result<Vec<ManaChoice>, EngineError> {
        let bindings = BTreeMap::new();
        let mut resolve = |value: &Value,
                           controller: &str,
                           _visited: &BTreeSet<String>,
                           _source: Option<&CardInstance>| {
            let context =
                mana_projection_context::ManaProjectionContext::for_value_interpretation(self);
            let scalar = |expression: &Value| {
                numeric_evaluation::evaluate_checked_numeric_tree(
                    expression,
                    &mut |leaf| {
                        if value_kind(leaf) == Some("countAttachments")
                            && matches!(value_kind(&leaf["object"]), Some("self" | "abilitySource"))
                            && source_lki::source_has_departed(runtime_source)
                        {
                            // Source snapshots capture P/T, counters and traits,
                            // not a historical attachment-set cardinality.
                            return None;
                        }
                        if matches!(value_kind(leaf), Some("powerOf" | "toughnessOf"))
                            && matches!(value_kind(&leaf["object"]), Some("self" | "abilitySource"))
                        {
                            let is_creature = if source_lki::source_has_departed(runtime_source) {
                                self.resolve_damage_source_lki(runtime_source)
                                    .ok()?
                                    .properties
                                    .is_creature
                            } else {
                                let live = self
                                    .permanent_position(&runtime_source.card.instance_id)
                                    .map(|(p, c)| &self.state.players[p].battlefield[c])?;
                                self.permanent_is_creature(live)
                            };
                            if !is_creature {
                                return None;
                            }
                        }
                        if value_kind(leaf) == Some("countCounters")
                            && matches!(value_kind(&leaf["object"]), Some("self" | "abilitySource"))
                        {
                            let live = self
                                .permanent_position(&runtime_source.card.instance_id)
                                .map(|(p, c)| &self.state.players[p].battlefield[c]);
                            let source = source_lki::source_object_input(runtime_source, live).card;
                            return leaf["counter"].as_str().map_or_else(
                                || {
                                    source
                                        .counters
                                        .values()
                                        .try_fold(0_i32, |sum, n| sum.checked_add(*n))
                                },
                                |name| Some(source.counters.get(name).copied().unwrap_or(0)),
                            );
                        }
                        self.runtime_numeric_expression(
                            leaf,
                            runtime_source,
                            &bindings,
                            &runtime_source.decisions,
                        )
                    },
                    &mut |condition| {
                        Some(self.condition_value(condition, controller, &runtime_source.decisions))
                    },
                )
            };
            match context
                .executable_outputs_with_scalar(value, controller, &runtime_source.card, &scalar)
                .map_err(|failure| failure.into_engine_error())?
            {
                ExecutableManaOutputs::NoMana(_) => Ok(vec![Vec::new()]),
                ExecutableManaOutputs::Choices(options) if options.is_empty() => Err(
                    EngineError::new("post-cost mana alternatives are unexpectedly empty"),
                ),
                ExecutableManaOutputs::Choices(options) => Ok(options),
            }
        };
        let raw = if let Some(rule) = rule {
            resolve(
                &rule["effects"][0]["mana"],
                &runtime_source.controller,
                &BTreeSet::new(),
                Some(&runtime_source.card),
            )?
        } else {
            vec![declared.intrinsic_symbols.clone().ok_or_else(|| {
                EngineError::new("intrinsic mana production has no typed base symbols")
            })?]
        };
        let choices = raw
            .into_iter()
            .map(|symbols| {
                let mut choice = declared.clone();
                choice.symbols = symbols;
                choice
            })
            .collect();
        self.finish_mana_production_choices(
            &runtime_source.card,
            &runtime_source.controller,
            choices,
            false,
            &mut resolve,
        )
    }

    pub(super) fn select_postcost_mana_choice(
        &self,
        rule: Option<&Value>,
        declared: &ManaChoice,
        runtime_source: &StackObject,
        selected_symbols: &[String],
    ) -> Result<ManaChoice, EngineError> {
        let mut choices = self.resolve_postcost_mana_choices(rule, declared, runtime_source)?;
        // A single semantic result has no unresolved player choice. Its amount
        // can legitimately become zero or its color combination can change.
        if choices.len() == 1 {
            return Ok(choices.remove(0));
        }
        choices.into_iter().find(|choice| choice.symbols == selected_symbols)
            .ok_or_else(|| EngineError::new(
                "post-cost mana choice requires a preserved declaration; preview output is not a new choice",
            ))
    }
}
