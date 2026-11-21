//! One fallible mana-payment path shared by result-producing control and
//! existing optional cost/effect control. Preparation is read-only.
use super::*;
use crate::engine::mana_discovery_session::ManaDiscoverySession;

/// Bounds a counted repetition of one mana cost (for example age counters).
const MAX_REPEATED_RESOLUTION_PAYMENTS: i32 = 1_000;

pub(in crate::engine) struct PreparedManaPayment {
    payer_id: String,
    player_index: usize,
    oracle_cost: String,
    plan: Option<ManaPayment>,
    /// A later part of a paid cost sequence: it cannot be declined.
    mandatory: bool,
}

impl PreparedManaPayment {
    pub(super) fn payable(&self) -> bool {
        self.plan.is_some()
    }

    pub(super) fn forbid_payment(&mut self) {
        self.plan = None;
    }

    pub(super) fn require_payment(&mut self) {
        self.mandatory = true;
    }
}

impl GameEngine {
    pub(in crate::engine) fn prepare_resolution_mana_payment(
        &self,
        payer_id: &str,
        cost: ResolutionManaCost,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<PreparedManaPayment, EngineError> {
        let player_index = self.player_index(payer_id)?;
        let (oracle_cost, requirement) = match cost {
            ResolutionManaCost::Fixed { label, requirement } => (label, requirement),
            ResolutionManaCost::Repeated {
                label,
                requirement,
                times,
            } => {
                let times = self
                    .checked_runtime_numeric_expression(
                        &times,
                        source,
                        bindings,
                        decisions,
                        "repeated payment count",
                    )?
                    .max(0);
                if times > MAX_REPEATED_RESOLUTION_PAYMENTS {
                    return Err(EngineError::new(
                        "repeated resolution mana payment exceeds its supported bound",
                    ));
                }
                let mut total = ManaRequirement::default();
                for _ in 0..times {
                    total.add(&requirement);
                }
                // This string is a decision label only; the planner receives typed mana.
                (label.repeat(times as usize), total)
            }
            ResolutionManaCost::Generic(amount) => {
                let amount = self
                    .checked_runtime_numeric_expression(
                        &amount,
                        source,
                        bindings,
                        decisions,
                        "numeric payment amount",
                    )?
                    .max(0);
                // This string is a decision label only; the planner receives typed mana.
                (
                    format!("{{{amount}}}"),
                    ManaRequirement {
                        generic: amount,
                        ..Default::default()
                    },
                )
            }
        };
        let plan = ManaDiscoverySession::new(self).payment_for_mana_requirement(
            player_index,
            requirement,
            &BTreeSet::new(),
            None,
        )?;
        Ok(PreparedManaPayment {
            payer_id: payer_id.to_string(),
            player_index,
            oracle_cost,
            plan,
            mandatory: false,
        })
    }

    pub(in crate::engine) fn choose_and_commit_resolution_mana_payment<P: DecisionProvider>(
        &mut self,
        prepared: PreparedManaPayment,
        scope_id: Option<&str>,
        source: &StackObject,
        provider: &mut P,
    ) -> Result<PaymentOutcome, EngineError> {
        let request_id = scope_id.map_or_else(
            || format!("optionalPayment:{}", source.id),
            |id| format!("costPayment:{}:{id}", source.id),
        );
        if prepared.mandatory && prepared.plan.is_none() {
            return Err(EngineError::new(
                "a mandatory resolution mana payment is no longer payable",
            ));
        }
        let mut options = vec![LegalAction {
            id: format!("{request_id}:decline"),
            kind: ActionKind::DeclinePayment,
            player_id: prepared.payer_id.clone(),
            label: "Decline payment".into(),
            card_instance_id: Some(source.card.instance_id.clone()),
            payment_sources: Vec::new(),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        }];
        if prepared.mandatory {
            options.clear();
        }
        if let Some(plan) = &prepared.plan {
            options.push(LegalAction {
                id: format!("{request_id}:pay"),
                kind: ActionKind::ChooseResolution,
                player_id: prepared.payer_id.clone(),
                label: format!("Pay {}", prepared.oracle_cost),
                card_instance_id: Some(source.card.instance_id.clone()),
                payment_sources: plan
                    .activations
                    .iter()
                    .map(|activation| activation.source_id.clone())
                    .collect(),
                decisions: BTreeMap::from([
                    ("manaPayment".to_string(), mana_payment_decision(plan)),
                    ("manaPoolPayment".to_string(), json!(plan.pool_indices)),
                ]),
                targets: BTreeMap::new(),
                target_order: Vec::new(),
                attacker_id: None,
                blocker_id: None,
            });
        }
        let action = self.choose_action(
            provider,
            EngineDecisionRequest {
                id: request_id,
                kind: DecisionKind::ResolutionChoice,
                player_id: prepared.payer_id,
                source_card: Some(source.card.clone()),
                source_card_instance_id: Some(source.card.instance_id.clone()),
                choice: None,
                options,
            },
        )?;
        if action.kind == ActionKind::DeclinePayment {
            return Ok(if prepared.plan.is_some() {
                PaymentOutcome::Declined
            } else {
                PaymentOutcome::Unavailable
            });
        }
        self.validate_generated_mana_payment(&action)?;
        for tapped in self.apply_payment(prepared.player_index, &action)? {
            self.enqueue_permanent_tapped_triggers(&tapped);
        }
        Ok(PaymentOutcome::Paid)
    }
}
