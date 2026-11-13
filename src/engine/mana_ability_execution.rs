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
    match rule["effects"].as_array().map(Vec::as_slice) {
        Some([effect]) if value_kind(effect) == Some("addMana") => {
            AutomaticManaPaymentCapability::PureProduction
        }
        _ => AutomaticManaPaymentCapability::NeedsOrderedExecution,
    }
}

impl GameEngine {
    /// Executes the non-mana primitives of a mana ability immediately, without
    /// using the stack. Mana selection and production remain owned by the
    /// existing mana-action path; this adapter preserves the remaining printed
    /// effect order after production.
    pub(super) fn execute_mana_ability_followup_effects<P: DecisionProvider>(
        &mut self,
        rule: &Value,
        action: &LegalAction,
        source: &CardInstance,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let effects = rule["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|effect| value_kind(effect) != Some("addMana"))
            .cloned()
            .collect::<Vec<_>>();
        if effects.is_empty() {
            return Ok(());
        }

        let runtime_source = StackObject {
            id: format!("mana-ability:{}", action.id),
            controller: action.player_id.clone(),
            card: source.clone(),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("manaAbility".to_string()),
            ability_rule: None,
            decisions: action.decisions.clone(),
            targets: action.targets.clone(),
        };
        let mut bindings = BTreeMap::new();
        let mut decisions = action.decisions.clone();
        self.execute_effects(
            &Value::Array(effects),
            &runtime_source,
            &mut bindings,
            &mut decisions,
            provider,
        )
    }
}
