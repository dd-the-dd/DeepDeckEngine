//! Scoped, ordered replacement programs for event quantities.
//!
//! A quantity leaf is meaningful only during this event. Source predicates,
//! recipient predicates and arithmetic stay separate from the primitive that
//! eventually creates tokens or places counters. No Oracle text is interpreted.

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::numeric_evaluation::{IntegerRounding, checked_integer_arithmetic};
use super::{
    ActionKind, CardDefinition, CardInstance, DecisionKind, DecisionProvider,
    EngineDecisionRequest, EngineError, GameEngine, LegalAction, card_filter_supported,
    matches_card_filter, permanent_filter_supported, value_kind,
};

/// Private engine checkpoint scope. It is neither game state nor Oracle IR.
/// The counter is test-only and records only global checkpoints owned here.
#[derive(Clone, Default)]
pub(super) struct QuantityTransactionState {
    in_progress: bool,
    failure: Option<String>,
    preflight_open: bool,
    #[cfg(test)]
    preflights: usize,
    #[cfg(test)]
    snapshots: usize,
}

/// Compatibility adapter for engine entry points whose caller did not supply
/// a chooser. It can execute deterministic replacements but refuses ambiguity.
pub(super) struct OptionalQuantityProvider<'a>(pub Option<&'a mut dyn DecisionProvider>);
impl DecisionProvider for OptionalQuantityProvider<'_> {
    fn choose(
        &mut self,
        state: &super::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.0
            .as_mut()
            .ok_or_else(|| {
                EngineError::new("quantity replacement requires an explicit decision provider")
            })?
            .choose(state, request)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QuantityOrigin {
    Effect,
    Cost,
    RuleAction,
}

#[derive(Clone, Copy)]
pub(super) enum QuantityRecipient<'a> {
    Permanent {
        permanent: &'a CardInstance,
        counter: &'a str,
    },
    Token {
        controller: &'a str,
        definition: &'a CardDefinition,
    },
}

impl<'a> QuantityRecipient<'a> {
    fn controller(self) -> &'a str {
        match self {
            Self::Permanent { permanent, .. } => &permanent.controller,
            Self::Token { controller, .. } => controller,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct QuantityEvent<'a> {
    pub amount: i32,
    pub origin: QuantityOrigin,
    pub recipient: QuantityRecipient<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct QuantityResolution {
    pub amount: i32,
    pub original_origin: QuantityOrigin,
    pub effective_origin: QuantityOrigin,
    /// Stable identities, including separate identical modifiers on one source.
    pub applied: Vec<String>,
}

/// Arithmetic over one event's quantity, shared by every event-quantity
/// replacement (counters, tokens, damage).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum QuantityExpression {
    EventQuantity,
    Integer {
        value: i32,
    },
    Add {
        left: Box<Self>,
        right: Box<Self>,
    },
    Subtract {
        left: Box<Self>,
        right: Box<Self>,
    },
    Multiply {
        left: Box<Self>,
        right: Box<Self>,
    },
    Divide {
        left: Box<Self>,
        right: Box<Self>,
        round: IntegerRounding,
    },
}

impl QuantityExpression {
    pub(super) fn evaluate(&self, quantity: i32) -> Result<i32, EngineError> {
        let (operation, left, right, round) = match self {
            Self::EventQuantity => return Ok(quantity),
            Self::Integer { value } => return Ok(*value),
            Self::Add { left, right } => ("add", left, right, None),
            Self::Subtract { left, right } => ("subtract", left, right, None),
            Self::Multiply { left, right } => ("multiply", left, right, None),
            Self::Divide { left, right, round } => ("divide", left, right, Some(*round)),
        };
        checked_integer_arithmetic(
            operation,
            left.evaluate(quantity)?,
            right.evaluate(quantity)?,
            round,
        )
        .ok_or_else(|| {
            EngineError::new(
                "quantity replacement arithmetic is undefined or exceeds its integer bound",
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum EventKind {
    CounterPlacement,
    TokenCreation,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OriginFilter {
    Any,
    Effect,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EventPattern {
    kind: EventKind,
    origin: OriginFilter,
    minimum: i32,
    player: Value,
    #[serde(default)]
    recipient_kind: Option<String>,
    #[serde(default, rename = "where")]
    predicate: Option<Value>,
    #[serde(default)]
    counter: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    kind: String,
    event: EventPattern,
    quantity: QuantityExpression,
}

fn source_player_pattern_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "eachPlayer" | "controllerOfAttachedPermanent") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("opponentsOf") => source_player_pattern_supported(&reference["player"]),
        _ => false,
    }
}

fn no_external_numeric_bindings(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            !matches!(
                value_kind(value),
                Some("decisionResult" | "sourceCastXValue" | "eventQuantity")
            ) && object.values().all(no_external_numeric_bindings)
        }
        Value::Array(values) => values.iter().all(no_external_numeric_bindings),
        _ => true,
    }
}

fn parse_program(modifier: &Value) -> Option<Program> {
    let program: Program = serde_json::from_value(modifier.clone()).ok()?;
    if program.kind != "replaceEventQuantity"
        || program.event.minimum < 1
        || !source_player_pattern_supported(&program.event.player)
        || program.event.predicate.as_ref().is_some_and(|filter| {
            let supported = match program.event.kind {
                EventKind::CounterPlacement => permanent_filter_supported(filter),
                EventKind::TokenCreation => card_filter_supported(filter),
            };
            !supported || !no_external_numeric_bindings(filter)
        })
    {
        return None;
    }
    match program.event.kind {
        EventKind::CounterPlacement => {
            if program.event.recipient_kind.as_deref() != Some("permanent")
                || program
                    .event
                    .counter
                    .as_ref()
                    .is_some_and(|name| name.trim().is_empty())
            {
                return None;
            }
        }
        EventKind::TokenCreation => {
            if program.event.recipient_kind.is_some() || program.event.counter.is_some() {
                return None;
            }
        }
    }
    Some(program)
}

pub(super) fn quantity_modifier_supported(modifier: &Value) -> Option<bool> {
    (value_kind(modifier) == Some("replaceEventQuantity"))
        .then(|| parse_program(modifier).is_some())
}

struct ApplicableProgram {
    id: String,
    source: CardInstance,
    program: Program,
}

impl GameEngine {
    /// Void observer adapters retain the FIRST error in the existing outer
    /// transaction. This is an abort, never a successful zero quantity.
    pub(super) fn defer_quantity_failure(&mut self, error: EngineError) {
        if self.quantity_transaction.failure.is_none() {
            self.quantity_transaction.failure = Some(error.to_string());
        }
    }

    pub(super) fn check_quantity_failure(&self) -> Result<(), EngineError> {
        match &self.quantity_transaction.failure {
            Some(message) => Err(EngineError::new(message.clone())),
            None => Ok(()),
        }
    }

    pub(super) fn quantity_transaction_in_progress(&self) -> bool {
        self.quantity_transaction.in_progress
    }

    /// The outermost active-quantity producer owns one checkpoint. Nested
    /// producers borrow that scope; their caller must propagate every error.
    pub(super) fn with_quantity_transaction<T>(
        &mut self,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        self.with_quantity_preflight(false, work)
    }

    pub(super) fn quantity_preflight_in_progress(&self) -> bool {
        self.quantity_transaction.preflight_open
    }

    /// One conservative analysis at the outer program boundary, including a
    /// passive scope when analysis proves that no checkpoint is required.
    pub(super) fn with_quantity_preflight<T>(
        &mut self,
        extra_risk: bool,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        self.check_quantity_failure()?;
        if self.quantity_transaction.in_progress {
            return self.with_replacement_checkpoint(false, work);
        }
        if self.quantity_transaction.preflight_open {
            return work(self);
        }
        #[cfg(test)]
        {
            self.quantity_transaction.preflights =
                self.quantity_transaction.preflights.saturating_add(1);
        }
        let required = extra_risk || self.quantity_known_sources_may_enable();
        self.quantity_transaction.preflight_open = true;
        let result = self.with_replacement_checkpoint(required, work);
        self.quantity_transaction.preflight_open = false;
        result
    }

    #[cfg(test)]
    pub(super) fn quantity_preflight_count(&self) -> usize {
        self.quantity_transaction.preflights
    }

    /// Reuse a checkpoint already required by linked movement or source-cost
    /// atomicity as well. This creates no new transaction for unrelated work.
    pub(super) fn with_replacement_checkpoint<T>(
        &mut self,
        required: bool,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        self.check_quantity_failure()?;
        if self.quantity_transaction.in_progress {
            let result = work(self).and_then(|value| {
                self.check_quantity_failure()?;
                Ok(value)
            });
            if let Err(error) = &result
                && self.quantity_transaction.failure.is_none()
            {
                self.quantity_transaction.failure = Some(error.to_string());
            }
            return result;
        }
        if !required {
            return work(self).and_then(|value| {
                self.check_quantity_failure()?;
                Ok(value)
            });
        }
        let checkpoint = super::resolution_checkpoint::ResolutionCheckpoint::capture(self);
        #[cfg(test)]
        let snapshots = self.quantity_transaction.snapshots.saturating_add(1);
        self.quantity_transaction.in_progress = true;
        self.quantity_transaction.failure = None;
        let mut result = work(self);
        if result.is_ok()
            && let Some(failure) = self.quantity_transaction.failure.take()
        {
            result = Err(EngineError::new(format!(
                "replacement transaction aborted after nested failure: {failure}"
            )));
        }
        if result.is_err() {
            checkpoint.rollback(self);
        }
        self.quantity_transaction.in_progress = false;
        self.quantity_transaction.failure = None;
        #[cfg(test)]
        {
            self.quantity_transaction.snapshots = snapshots;
        }
        result
    }

    #[cfg(test)]
    pub(super) fn quantity_snapshot_count(&self) -> usize {
        self.quantity_transaction.snapshots
    }

    pub(super) fn quantity_replacements_active(&self) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(_, m)| value_kind(m) == Some("replaceEventQuantity"))
    }

    /// Preserve the stack source and prepared objects if the provider refuses
    /// a replacement or permission. Ordinary deterministic programs retain
    /// the preflight path without an unconditional engine clone.
    pub(super) fn resolve_top_stack<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let resolution_risk =
            super::resolution_preflight::stack_requires_checkpoint(&self.state.stack);
        self.with_quantity_preflight(resolution_risk, |engine| {
            engine.resolve_top_stack_quantity_inner(provider)
        })
    }

    pub(super) fn resolve_combat_damage_step_with_provider(
        &mut self,
        attacks: &super::BTreeMap<String, super::AttackAssignment>,
        blocks: &super::BTreeMap<String, Vec<String>>,
        assignments: &super::BTreeMap<String, super::BTreeMap<String, i32>>,
        step: super::CombatDamageStep,
        provider: Option<&mut dyn DecisionProvider>,
    ) -> Result<(), EngineError> {
        self.with_quantity_transaction(|engine| {
            engine.resolve_combat_damage_step_quantity_inner(
                attacks,
                blocks,
                assignments,
                step,
                provider,
            )
        })
    }

    fn quantity_program_matches(
        &self,
        candidate: &ApplicableProgram,
        event: QuantityEvent<'_>,
        amount: i32,
        effective_origin: QuantityOrigin,
    ) -> bool {
        let pattern = &candidate.program.event;
        if amount < pattern.minimum
            || (matches!(pattern.origin, OriginFilter::Effect)
                && effective_origin != QuantityOrigin::Effect)
            || !self.event_player_reference_matches(
                &pattern.player,
                &candidate.source,
                event.recipient.controller(),
            )
        {
            return false;
        }
        match (pattern.kind, event.recipient) {
            (EventKind::CounterPlacement, QuantityRecipient::Permanent { permanent, counter }) => {
                pattern
                    .counter
                    .as_ref()
                    .is_none_or(|wanted| wanted == counter)
                    && pattern.predicate.as_ref().is_none_or(|filter| {
                        self.permanent_matches_effective_filter(permanent, filter)
                    })
            }
            (EventKind::TokenCreation, QuantityRecipient::Token { definition, .. }) => pattern
                .predicate
                .as_ref()
                .is_none_or(|filter| matches_card_filter(definition, filter)),
            _ => false,
        }
    }

    /// Resolve the affected controller's order before the actual producer mutates.
    /// A refused choice leaves the engine's branch accounting unchanged as well.
    pub(super) fn resolve_quantity_event<P: DecisionProvider>(
        &self,
        event: QuantityEvent<'_>,
        provider: &mut P,
    ) -> Result<QuantityResolution, EngineError> {
        if event.amount <= 0 {
            return Ok(QuantityResolution {
                amount: 0,
                original_origin: event.origin,
                effective_origin: event.origin,
                applied: Vec::new(),
            });
        }
        let prior_sequence = self.branching_decision_sequence.get();
        let result = self.resolve_quantity_event_inner(event, provider);
        if result.is_err() {
            self.branching_decision_sequence.set(prior_sequence);
        }
        result
    }

    fn resolve_quantity_event_inner<P: DecisionProvider>(
        &self,
        event: QuantityEvent<'_>,
        provider: &mut P,
    ) -> Result<QuantityResolution, EngineError> {
        let programs = self
            .battlefield_static_modifiers()
            .into_iter()
            .enumerate()
            .filter_map(|(ordinal, (source, modifier))| {
                if value_kind(&modifier) != Some("replaceEventQuantity") {
                    return None;
                }
                let program = parse_program(&modifier)?;
                Some(ApplicableProgram {
                    id: format!("{}:quantity:{ordinal}", source.instance_id),
                    source,
                    program,
                })
            })
            .collect::<Vec<_>>();
        let mut used = BTreeSet::new();
        let mut resolution = QuantityResolution {
            amount: event.amount,
            original_origin: event.origin,
            effective_origin: event.origin,
            applied: Vec::new(),
        };
        while resolution.amount > 0 {
            let applicable = programs
                .iter()
                .enumerate()
                .filter(|(index, program)| {
                    !used.contains(index)
                        && self.quantity_program_matches(
                            program,
                            event,
                            resolution.amount,
                            resolution.effective_origin,
                        )
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if applicable.is_empty() {
                break;
            }
            let index = if applicable.len() == 1 {
                applicable[0]
            } else {
                let player = event.recipient.controller().to_string();
                let request = EngineDecisionRequest {
                    id: format!(
                        "quantity-replacement:{}:{}:{}",
                        self.state.turn_number,
                        self.state.events.len(),
                        resolution.applied.len()
                    ),
                    kind: DecisionKind::ReplacementChoice,
                    player_id: player.clone(),
                    source_card: None,
                    source_card_instance_id: None,
                    choice: None,
                    options: applicable
                        .iter()
                        .map(|index| {
                            let replacement = &programs[*index];
                            LegalAction {
                                id: replacement.id.clone(),
                                kind: ActionKind::ChooseResolution,
                                player_id: player.clone(),
                                label: format!("Apply {}", replacement.source.definition.name),
                                card_instance_id: Some(replacement.source.instance_id.clone()),
                                payment_sources: Vec::new(),
                                decisions: BTreeMap::from([(
                                    "quantityReplacementId".to_string(),
                                    json!(replacement.id),
                                )]),
                                targets: BTreeMap::new(),
                                target_order: Vec::new(),
                                attacker_id: None,
                                blocker_id: None,
                            }
                        })
                        .collect(),
                };
                let choice = self.choose_action(provider, request)?;
                applicable
                    .into_iter()
                    .find(|index| programs[*index].id == choice.id)
                    .ok_or_else(|| {
                        EngineError::new("quantity replacement choice has no matching program")
                    })?
            };
            let replacement = &programs[index];
            resolution.amount = replacement
                .program
                .quantity
                .evaluate(resolution.amount)?
                .max(0);
            // CR614.16/616.2: once a replacement creates the modified token or
            // counter event, an effect-only replacement may newly apply even
            // when the original event was a cost or a turn-based action.
            resolution.effective_origin = QuantityOrigin::Effect;
            used.insert(index);
            resolution.applied.push(replacement.id.clone());
        }
        Ok(resolution)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "quantity_replacement_tests.rs"]
mod tests;
