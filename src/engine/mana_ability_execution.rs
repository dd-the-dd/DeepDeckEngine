use super::*;

/// Automatic payment currently projects production without resolving an
/// ability program. Keep such projections distinct from explicit mana
/// activations, whose normal executor can perform followups and choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AutomaticManaPaymentCapability {
    PureProduction,
    NeedsActivationDecisions,
    NeedsOrderedExecution,
}

impl AutomaticManaPaymentCapability {
    pub(super) fn is_safe(self) -> bool {
        self == Self::PureProduction
    }
}

pub(super) fn automatic_payment_capability(rule: &Value) -> AutomaticManaPaymentCapability {
    if rule["declaration"]["decisions"]
        .as_array()
        .is_some_and(|decisions| !decisions.is_empty())
        || rule["costs"].as_array().is_some_and(|costs| {
            costs.iter().any(|cost| {
                value_kind(cost) == Some("payMana")
                    && cost["manaCost"].as_str().is_some_and(|mana_cost| {
                        parse_mana_symbols(mana_cost)
                            .iter()
                            .any(|symbol| symbol == "X")
                    })
            })
        })
    {
        return AutomaticManaPaymentCapability::NeedsActivationDecisions;
    }
    if rule["costs"].as_array().into_iter().flatten().any(|cost| {
        matches!(
            value_kind(cost),
            Some("putCounters" | "putCountersOnSource")
        )
    }) {
        return AutomaticManaPaymentCapability::NeedsOrderedExecution;
    }
    match rule["effects"].as_array().map(Vec::as_slice) {
        Some([effect]) if value_kind(effect) == Some("addMana") => {
            AutomaticManaPaymentCapability::PureProduction
        }
        _ => AutomaticManaPaymentCapability::NeedsOrderedExecution,
    }
}

/// Share the exact followup selection between risk detection and execution.
/// Main direct production remains owned by the ordinary mana action path.
pub(super) fn followup_effects(rule: &Value) -> impl Iterator<Item = &Value> {
    rule["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|effect| value_kind(effect) != Some("addMana"))
}

impl GameEngine {
    pub(super) fn immediate_mana_action_requires_checkpoint(&self, action: &LegalAction) -> bool {
        if action.kind != ActionKind::ActivateAbility {
            return false;
        }
        let off_battlefield = action.decisions.get("offBattlefieldManaProduced").is_some();
        if action.decisions.get("manaAbility").and_then(Value::as_bool) != Some(true)
            && !off_battlefield
        {
            return false;
        }
        let granted = granted_mana_program::declared_origin(action);
        if granted.is_err() {
            return true;
        }
        let granted = granted.ok().flatten();
        let selected = if let Some(ManaProgramOrigin::Granted { program, .. }) = &granted {
            Some(program)
        } else if let Some(rule) = action.decisions.get("grantedAbilityRule") {
            Some(rule)
        } else {
            let Some(raw_index) = action.decisions.get("abilityRuleIndex") else {
                return off_battlefield || self.mana_production_requires_postcost(None);
            };
            let Some(index) = raw_index.as_u64().and_then(|n| usize::try_from(n).ok()) else {
                return true;
            };
            let Some(id) = action.card_instance_id.as_deref() else {
                return true;
            };
            let source = self
                .permanent_position(id)
                .map(|(p, c)| &self.state.players[p].battlefield[c])
                .or_else(|| self.card_outside_battlefield(id));
            source.and_then(|card| card.definition.rules.get(index))
        };
        selected.is_none_or(|rule| {
            super::resolution_preflight::requires_checkpoint(rule)
                || self.mana_production_requires_postcost(Some(rule))
        })
    }

    /// The same immediate interpreter owns primary production and followups.
    /// In particular, never recreate a stale source snapshot after its costs.
    pub(super) fn execute_mana_followups_in_source_context<P: DecisionProvider>(
        &mut self,
        rule: &Value,
        action: &LegalAction,
        runtime_source: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let effects = followup_effects(rule).cloned().collect::<Vec<_>>();
        if effects.is_empty() {
            return Ok(());
        }
        let mut bindings = BTreeMap::new();
        let mut decisions = action.decisions.clone();
        self.execute_effects(
            &Value::Array(effects),
            runtime_source,
            &mut bindings,
            &mut decisions,
            provider,
        )
    }
}
