//! Shared resolution-time cost programs. Payment choice and cost commitment
//! are independent of the effect that consumes their Boolean result.
use super::*;

mod card_costs;
mod mana;
mod nonmana;

pub(in crate::engine) enum PreparedPayment {
    Mana(mana::PreparedManaPayment),
    NonMana(nonmana::PreparedNonManaPayment),
    /// Every part in payment order. Only the first part may be declined;
    /// later parts are mandatory once it is paid (CR 118.3, 601.2h).
    Sequence(Vec<PreparedPayment>),
}

#[derive(Clone, Debug)]
pub(super) enum CostProgram {
    Mana(ResolutionManaCost),
    NonMana(Value),
    /// `A and B`: all parts are paid or none. Parts draw on distinct
    /// resources, so their independent payability is joint payability.
    Sequence(Vec<CostProgram>),
}

/// The resource a resolution cost part consumes. Two parts of one sequence
/// must not share one: a land tapped for mana could not also be sacrificed,
/// and one graveyard card could not pay two exile parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CostResource {
    Hand,
    Graveyard,
    /// Mana, life and battlefield objects (mana abilities may consume any).
    Battlefield,
}

impl CostProgram {
    fn resource(&self) -> CostResource {
        match self {
            Self::NonMana(cost) => match value_kind(cost) {
                Some("discardCard") => CostResource::Hand,
                Some("exileCard") if cost["zone"].as_str() == Some("hand") => CostResource::Hand,
                Some("exileCard") => CostResource::Graveyard,
                _ => CostResource::Battlefield,
            },
            Self::Mana(_) | Self::Sequence(_) => CostResource::Battlefield,
        }
    }

    fn decode_sequence(cost: &Value) -> Result<Self, EngineError> {
        let parts = cost["costs"]
            .as_array()
            .filter(|parts| parts.len() >= 2)
            .ok_or_else(|| {
                EngineError::new("resolution cost sequence requires at least two parts")
            })?
            .iter()
            .map(|part| {
                if value_kind(part) == Some("costSequence") {
                    return Err(EngineError::new(
                        "unsupported resolution cost program: nested cost sequence",
                    ));
                }
                Self::decode(part)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut resources = parts.iter().map(Self::resource).collect::<Vec<_>>();
        resources.sort();
        if resources.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EngineError::new(
                "unsupported resolution cost program: two sequence parts share one resource",
            ));
        }
        // Card-zone parts are paid first: they cannot change which mana or
        // battlefield objects remain available to the final part.
        let mut parts = parts;
        parts.sort_by_key(|part| part.resource() == CostResource::Battlefield);
        Ok(Self::Sequence(parts))
    }
}

/// `exileCard`: one card the payer chooses from their hand or graveyard.
fn zone_card_exile_cost_supported(cost: &Value) -> bool {
    matches!(cost["zone"].as_str(), Some("hand" | "graveyard"))
        && cost
            .get("where")
            .is_some_and(|filter| filter.is_null() || card_filter_supported(filter))
        && cost.get("excludeSource").is_none_or(Value::is_boolean)
}

impl PreparedPayment {
    /// True when at least one way to pay every part exists.
    fn payable(&self) -> bool {
        match self {
            Self::Mana(payment) => payment.payable(),
            Self::NonMana(payment) => payment.payable(),
            Self::Sequence(parts) => parts.iter().all(Self::payable),
        }
    }

    /// Leaves only the decline option: the whole cost is unavailable.
    fn forbid_payment(&mut self) {
        match self {
            Self::Mana(payment) => payment.forbid_payment(),
            Self::NonMana(payment) => payment.forbid_payment(),
            Self::Sequence(parts) => parts.iter_mut().for_each(Self::forbid_payment),
        }
    }

    /// Removes the decline option: an earlier part of the cost was paid.
    fn require_payment(&mut self) {
        match self {
            Self::Mana(payment) => payment.require_payment(),
            Self::NonMana(payment) => payment.require_payment(),
            Self::Sequence(parts) => parts.iter_mut().for_each(Self::require_payment),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum ResolutionManaCost {
    Fixed {
        label: String,
        requirement: ManaRequirement,
    },
    Generic(Value),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaymentOutcome {
    Paid,
    Declined,
    Unavailable,
}

impl PaymentOutcome {
    pub(super) fn paid(self) -> bool {
        self == Self::Paid
    }
}

impl CostProgram {
    pub(super) fn decode(cost: &Value) -> Result<Self, EngineError> {
        if value_kind(cost) == Some("costSequence") {
            return Self::decode_sequence(cost);
        }
        if value_kind(cost) == Some("exileCard") {
            return if zone_card_exile_cost_supported(cost) {
                Ok(Self::NonMana(cost.clone()))
            } else {
                Err(EngineError::new(
                    "unsupported resolution cost program: invalid card exile cost",
                ))
            };
        }
        if value_kind(cost) != Some("payMana") {
            let supported = matches!(
                value_kind(cost),
                Some(
                    "payPlayerCounters"
                        | "putCounters"
                        | "removeCounters"
                        | "tap"
                        | "tapPermanents"
                        | "payWaterbend"
                        | "payLife"
                        | "sacrificePermanent"
                        | "discardCard"
                )
            ) && cost_supported(cost);
            let has_resource_selector = match value_kind(cost) {
                Some("sacrificePermanent") => {
                    cost.get("where").is_some()
                        || target_reference_supported(&cost["permanent"])
                        || matches!(
                            value_kind(&cost["permanent"]),
                            Some("self" | "abilitySource")
                        )
                }
                Some("discardCard") => {
                    cost.get("where").is_some()
                        || target_reference_supported(&cost["card"])
                        || matches!(value_kind(&cost["card"]), Some("self" | "abilitySource"))
                }
                _ => true,
            };
            return if supported && has_resource_selector {
                Ok(Self::NonMana(cost.clone()))
            } else {
                Err(EngineError::new(
                    "unsupported resolution cost program: cost form has no shared payment implementation",
                ))
            };
        }
        let fixed = cost.get("manaCost").filter(|value| !value.is_null());
        let dynamic = cost.get("genericAmount").filter(|value| !value.is_null());
        match (fixed, dynamic) {
            (Some(Value::String(value)), None) => {
                let requirement = parse_fixed_mana_requirement_checked(value).ok_or_else(|| {
                    EngineError::new("unsupported fixed resolution mana cost: bounded whole-mana symbols required")
                })?;
                Ok(Self::Mana(ResolutionManaCost::Fixed {
                    label: value.clone(),
                    requirement,
                }))
            }
            (None, Some(amount)) if numeric_expression_supported(amount) => {
                Ok(Self::Mana(ResolutionManaCost::Generic(amount.clone())))
            }
            _ => Err(EngineError::new(
                "invalid resolution mana cost: expected one bounded fixed cost or a typed generic amount",
            )),
        }
    }
}

impl GameEngine {
    pub(in crate::engine) fn prepare_resolution_cost_payment(
        &self,
        payer_id: &str,
        program: CostProgram,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<PreparedPayment, EngineError> {
        match program {
            CostProgram::Mana(cost) => self
                .prepare_resolution_mana_payment(payer_id, cost, source, bindings, decisions)
                .map(PreparedPayment::Mana),
            CostProgram::NonMana(cost) => self
                .prepare_resolution_nonmana_payment(payer_id, cost, source, bindings, decisions)
                .map(PreparedPayment::NonMana),
            CostProgram::Sequence(parts) => parts
                .into_iter()
                .map(|part| {
                    self.prepare_resolution_cost_payment(
                        payer_id, part, source, bindings, decisions,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map(PreparedPayment::Sequence),
        }
    }
    pub(in crate::engine) fn choose_and_commit_resolution_payment<P: DecisionProvider>(
        &mut self,
        prepared: PreparedPayment,
        scope_id: Option<&str>,
        source: &StackObject,
        provider: &mut P,
    ) -> Result<PaymentOutcome, EngineError> {
        match prepared {
            PreparedPayment::Mana(value) => {
                self.choose_and_commit_resolution_mana_payment(value, scope_id, source, provider)
            }
            PreparedPayment::NonMana(value) => {
                self.choose_and_commit_resolution_nonmana_payment(value, scope_id, source, provider)
            }
            PreparedPayment::Sequence(parts) => {
                self.choose_and_commit_resolution_cost_sequence(parts, scope_id, source, provider)
            }
        }
    }

    /// CR 118.3: an unpayable part makes the whole list unpayable, so the
    /// payer is offered only to decline. Otherwise the first part carries the
    /// pay-or-decline choice and each later part is a mandatory payment.
    fn choose_and_commit_resolution_cost_sequence<P: DecisionProvider>(
        &mut self,
        parts: Vec<PreparedPayment>,
        scope_id: Option<&str>,
        source: &StackObject,
        provider: &mut P,
    ) -> Result<PaymentOutcome, EngineError> {
        let payable = parts.iter().all(PreparedPayment::payable);
        let mut parts = parts.into_iter();
        let mut first = parts
            .next()
            .ok_or_else(|| EngineError::new("empty resolution cost sequence"))?;
        if !payable {
            first.forbid_payment();
        }
        let outcome =
            self.choose_and_commit_resolution_payment(first, scope_id, source, provider)?;
        if !outcome.paid() {
            return Ok(outcome);
        }
        for (index, mut part) in parts.enumerate() {
            part.require_payment();
            let part_scope = format!(
                "{}part{}",
                scope_id.map_or_else(String::new, |id| format!("{id}:")),
                index + 1
            );
            let outcome = self.choose_and_commit_resolution_payment(
                part,
                Some(&part_scope),
                source,
                provider,
            )?;
            if !outcome.paid() {
                return Err(EngineError::new(
                    "a mandatory part of a paid resolution cost sequence was not paid",
                ));
            }
        }
        Ok(PaymentOutcome::Paid)
    }
}

pub(super) fn cost_program_supported(cost: &Value) -> bool {
    CostProgram::decode(cost).is_ok()
}

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(
        value_kind(value),
        Some("tryPayCost" | "optionalPayCostPerformEffects" | "changeTargets")
    ) || (matches!(value_kind(value), Some("costWasPaid" | "decisionWas"))
        && value["required"].as_bool() == Some(true))
        || (matches!(
            value_kind(value),
            Some("copyStackItem" | "copyResolvingStackObject")
        ) && value["mayChooseNewTargets"].as_bool() == Some(true))
        || (value_kind(value) == Some("copyResolvingSpellIfCastFromZone")
            && value["chooseNewTargets"].as_bool() == Some(true))
}

/// A local result is required only when the compiler explicitly emits that
/// obligation. Legacy optional Boolean reads keep their established contract.
/// This checks bindings; it never evaluates conditions or chooses a branch.
pub(super) fn validate_required_results(
    condition: &Value,
    decisions: &BTreeMap<String, Value>,
) -> Result<(), EngineError> {
    fn visit(
        value: &Value,
        decisions: &BTreeMap<String, Value>,
        depth: usize,
    ) -> Result<(), EngineError> {
        if depth >= 64 {
            return Err(EngineError::new(
                "required result condition exceeds supported depth",
            ));
        }
        match value {
            Value::Object(fields) => {
                if matches!(value_kind(value), Some("costWasPaid" | "decisionWas"))
                    && value["required"].as_bool() == Some(true)
                {
                    let id = value["decisionId"].as_str().ok_or_else(|| {
                        EngineError::new("required cost result has no decision id")
                    })?;
                    if decisions.get(id).and_then(Value::as_bool).is_none() {
                        return Err(EngineError::new(format!(
                            "missing or non-Boolean required cost result {id}"
                        )));
                    }
                }
                for child in fields.values() {
                    visit(child, decisions, depth + 1)?;
                }
            }
            Value::Array(children) => {
                for child in children {
                    visit(child, decisions, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(condition, decisions, 0)
}

/// Generic control builder for runtime-created abilities. The caller supplies
/// the actor and both independent continuation programs; this has no keyword logic.
pub(super) fn optional_cost_program(
    id: &str,
    player: Value,
    cost: Value,
    paid: Vec<Value>,
    declined: Vec<Value>,
) -> Vec<Value> {
    vec![
        json!({"kind":"tryPayCost","id":id,"player":player,"cost":cost}),
        json!({
            "kind":"conditional",
            "condition":{"kind":"costWasPaid","decisionId":id,"value":true,"required":true},
            "then":paid,
            "else":declined,
        }),
    ]
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "tests.rs"]
mod tests;
