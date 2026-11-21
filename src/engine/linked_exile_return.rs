//! Immediate zone transitions linked to one battlefield incarnation.
//!
//! Pending transitions live in GameState's private runtime slots, so action
//! and payment snapshots restore them together with the zones and links.
//! They are not executable IR modifiers or counterable stack objects.

use super::*;
use serde::{Deserialize, Serialize};

const PENDING_KIND: &str = "pendingLinkedExileReturn";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LinkedReturnCard {
    card_id: String,
    exile_identity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum PendingLinkedReturn {
    Ready { entries: Vec<LinkedReturnCard> },
    UnsupportedHistoricalIdentity { source_id: String, card_id: String },
    UnsupportedDestination { destination: String },
}

impl GameEngine {
    fn action_can_finish_linked_duration(&self, action: &LegalAction) -> bool {
        if !matches!(
            action.kind,
            ActionKind::CastSpell | ActionKind::ActivateAbility
        ) {
            return false;
        }
        self.state.rule_modifiers.iter().any(|modifier| {
            if value_kind(modifier) != Some("exiledWithSource")
                || modifier["returnWhenSourceLeaves"].as_bool() != Some(true)
            {
                return false;
            }
            let Some(source) = modifier["sourceCardInstanceId"].as_str() else {
                return false;
            };
            action.card_instance_id.as_deref() == Some(source)
                || action
                    .payment_sources
                    .iter()
                    .any(|candidate| candidate == source)
                || action
                    .decisions
                    .values()
                    .any(|value| value_contains_string(value, source))
                || action.targets.values().any(|target| match target {
                    TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                        instance_id == source
                    }
                    TargetRef::Player { .. } | TargetRef::StackObject { .. } => false,
                })
        })
    }

    /// Ordered payment programs and actions referencing active linked sources
    /// need an outer snapshot. Unrelated nonordered casts do not clone it.
    pub(super) fn apply_priority_action<P: DecisionProvider>(
        &mut self,
        action: &LegalAction,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.validate_static_casting_base_permission(action)?;
        self.validate_action_target_cost_programs(action)?;
        // An ordered payment is a fallible state-changing program. Promote
        // even an already-open passive preflight before its first source cost;
        // active engine-wide transactions borrow their existing checkpoint.
        let ordered_payment = mana_postcost_ordered::is_ordered_payment(&action.decisions);
        // A cast/land play changes the physical source's zone; activated
        // programs may pay zone-changing costs. No identity reservation may
        // span those costs/callbacks, so the outer operation owns rollback.
        // This conservative admission is measured separately from execution.
        let identity_transition = matches!(
            action.kind,
            ActionKind::CastSpell | ActionKind::PlayLand | ActionKind::ActivateAbility
        );
        self.with_replacement_checkpoint(ordered_payment || identity_transition, |engine| {
            if matches!(
                action.kind,
                ActionKind::CastSpell | ActionKind::ActivateAbility | ActionKind::PlayLand
            ) {
                let extra_risk = if engine.quantity_transaction_in_progress()
                    || engine.quantity_preflight_in_progress()
                {
                    false
                } else {
                    engine.quantity_action_payload_may_enable(action)
                        || engine.immediate_mana_action_requires_checkpoint(action)
                };
                let linked_checkpoint = engine.action_can_finish_linked_duration(action);
                return engine.with_quantity_preflight(extra_risk || linked_checkpoint, |engine| {
                    engine.apply_priority_action_inner(action, provider)
                });
            }
            let required = engine.action_can_finish_linked_duration(action);
            engine.with_quantity_preflight(required, |engine| {
                engine.apply_priority_action_inner(action, provider)
            })
        })
    }

    fn linked_return_movement() -> Value {
        json!({
            "kind": "moveCards",
            "cards": { "kind": "boundObjects", "binding": "linkedReturnCards" },
            "to": {
                "kind": "battlefield", "tapped": false,
                "player": { "kind": "ownerOf", "object": { "kind": "candidate" } },
            },
        })
    }
    fn linked_source_identity(source: &CardInstance) -> Option<&str> {
        Self::tracked_zone_identity(source)
    }

    pub(super) fn linked_source_is_same_battlefield_object(&self, source: &CardInstance) -> bool {
        self.permanent_position(&source.instance_id)
            .is_some_and(|(player, index)| {
                Self::linked_source_identity(&self.state.players[player].battlefield[index])
                    .zip(Self::linked_source_identity(source))
                    .is_some_and(|(live, captured)| live == captured)
            })
    }

    /// Return the marked live snapshot so entry-trigger stack objects carry
    /// the incarnation that actually entered, including after a blink.
    #[cfg(test)]
    pub(super) fn mark_linked_battlefield_entry(&mut self, source: &CardInstance) -> CardInstance {
        let mut prepared = source.clone();
        self.prepare_battlefield_entry_identity(&mut prepared);
        let identity = Self::tracked_zone_identity(&prepared)
            .expect("test construction prepared identity")
            .to_string();
        if let Some((player, index)) = self.permanent_position(&source.instance_id) {
            let live = &mut self.state.players[player].battlefield[index];
            Self::assign_tracked_zone_identity(live, &identity);
            return live.clone();
        }
        source.clone()
    }

    pub(super) fn register_linked_exiled_permanent(
        &mut self,
        source: &CardInstance,
        exiled_id: &str,
        return_when_source_leaves: bool,
        last_known_definition: &CardDefinition,
    ) -> Result<(), EngineError> {
        let mut record = json!({
            "kind": "exiledWithSource", "sourceCardInstanceId": source.instance_id,
            "cardInstanceId": exiled_id, "returnWhenSourceLeaves": return_when_source_leaves,
            "lastKnownDefinition": last_known_definition,
        });
        if return_when_source_leaves {
            let card = self
                .state
                .players
                .iter_mut()
                .flat_map(|player| player.exile.iter_mut())
                .find(|card| card.instance_id == exiled_id)
                .ok_or_else(|| {
                    EngineError::new("linked exile requires the moved object in exile")
                })?;
            let identity = super::object_identity::object_identity_of(card)?.zone_identity;
            super::object_identity::object_identity_of(source)?;
            record["linkedExileIdentity"] = json!(identity);
            record["linkedSourceIdentity"] = json!(Self::linked_source_identity(source));
        }
        self.state.rule_modifiers.push(record);
        // The source may itself have left during the same primitive. Consume
        // that duration now instead of leaving a link that can never wake up.
        if return_when_source_leaves && !self.linked_source_is_same_battlefield_object(source) {
            self.queue_linked_exile_return(source);
        }
        Ok(())
    }

    fn push_pending_linked_return(&mut self, pending: PendingLinkedReturn) {
        self.state
            .rule_modifiers
            .push(json!({ "kind": PENDING_KIND, "payload": pending }));
    }

    pub(super) fn has_pending_linked_exile_returns(&self) -> bool {
        self.state
            .rule_modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some(PENDING_KIND))
    }

    /// Capture only this source incarnation, before any later primitive can
    /// create a different link. Physical return waits for the enclosing batch.
    pub(super) fn queue_linked_exile_return(&mut self, left: &CardInstance) {
        let mut entries = Vec::new();
        let mut failures = Vec::new();
        let identity = Self::linked_source_identity(left);
        self.state.rule_modifiers.retain(|modifier| {
            if value_kind(modifier) != Some("exiledWithSource")
                || modifier["sourceCardInstanceId"].as_str() != Some(left.instance_id.as_str())
                || modifier["returnWhenSourceLeaves"].as_bool() != Some(true)
            {
                return true;
            }
            let card_id = modifier["cardInstanceId"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if identity.is_none() {
                failures.push(PendingLinkedReturn::UnsupportedHistoricalIdentity {
                    source_id: left.instance_id.clone(),
                    card_id,
                });
                return false;
            }
            let (Some(source_identity), Some(exile_identity)) = (
                modifier["linkedSourceIdentity"].as_str(),
                modifier["linkedExileIdentity"].as_str(),
            ) else {
                failures.push(PendingLinkedReturn::UnsupportedHistoricalIdentity {
                    source_id: left.instance_id.clone(),
                    card_id,
                });
                return false;
            };
            if identity != Some(source_identity) {
                return true;
            }
            let destination = modifier["returnZone"].as_str().unwrap_or("battlefield");
            if destination != "battlefield" {
                failures.push(PendingLinkedReturn::UnsupportedDestination {
                    destination: destination.to_string(),
                });
                return false;
            }
            entries.push(LinkedReturnCard {
                card_id,
                exile_identity: exile_identity.to_string(),
            });
            false
        });
        if !entries.is_empty() {
            self.push_pending_linked_return(PendingLinkedReturn::Ready { entries });
        }
        for failure in failures {
            self.push_pending_linked_return(failure);
        }
    }

    pub(super) fn drain_linked_exile_returns<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.check_quantity_failure()?;
        if !self.has_pending_linked_exile_returns() {
            return Ok(());
        }
        self.with_replacement_checkpoint(true, |engine| {
            engine.drain_linked_exile_returns_inner(provider)
        })
    }

    fn drain_linked_exile_returns_inner<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        while self.has_pending_linked_exile_returns() {
            let mut entries = Vec::new();
            // One wave includes every duration ending in the same completed
            // zone batch, including links from several departing sources.
            while let Some(index) = self
                .state
                .rule_modifiers
                .iter()
                .position(|modifier| value_kind(modifier) == Some(PENDING_KIND))
            {
                let slot = self.state.rule_modifiers.remove(index);
                let pending: PendingLinkedReturn = serde_json::from_value(slot["payload"].clone())
                    .map_err(|_| EngineError::new("invalid private linked-return continuation"))?;
                match pending {
                    PendingLinkedReturn::Ready { entries: ready } => entries.extend(ready),
                    PendingLinkedReturn::UnsupportedHistoricalIdentity { source_id, card_id } => {
                        return Err(EngineError::new(format!(
                            "linked exile {source_id}/{card_id} has no authenticated zone incarnation"
                        )));
                    }
                    PendingLinkedReturn::UnsupportedDestination { destination } => {
                        return Err(EngineError::new(format!(
                            "unsupported linked return destination {destination}"
                        )));
                    }
                }
            }
            let mut moved = BTreeSet::new();
            let mut cards = Vec::new();
            for entry in entries {
                let located = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| &player.exile)
                    .find(|card| {
                        card.instance_id == entry.card_id
                            && Self::tracked_zone_identity(card)
                                == Some(entry.exile_identity.as_str())
                    });
                if let Some(card) = located
                    && self.nonbattlefield_card_can_change_zone(&entry.card_id)
                    && moved.insert(entry.card_id)
                {
                    cards.push(card.clone());
                }
            }
            let Some(first) = cards.first().cloned() else {
                continue;
            };
            let context = StackObject {
                id: format!(
                    "linked-return:{}:{}",
                    self.state.turn_number,
                    self.state.events.len()
                ),
                controller: first.owner.clone(),
                card: first,
                cant_be_countered: false,
                exile_on_leave_stack: false,
                ability_kind: Some("immediateZoneTransition".to_string()),
                ability_rule: None,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            };
            let movement = Self::linked_return_movement();
            if !effect_supported(&movement) {
                return Err(EngineError::new(
                    "the shared linked-return moveCards program is unsupported",
                ));
            }
            let mut bindings = BTreeMap::from([(
                "linkedReturnCards".to_string(),
                RuntimeBinding::Objects(cards.into_iter().map(|card| card.instance_id).collect()),
            )]);
            // Execution frame only, never inserted in GameState.stack.
            // Shared moveCards owns the owner-per-card destination and the
            // simultaneous battlefield publication; this domain owns links.
            self.execute_effect(
                &movement,
                None,
                &context,
                &mut bindings,
                &mut BTreeMap::new(),
                provider,
            )?;
        }
        Ok(())
    }

    /// Runtime callers supply the real decision provider. The old no-provider
    /// test boundary fails explicitly if an immediate return needs completion.
    pub(super) fn check_state_based_actions_with_provider(
        &mut self,
        provider: Option<&mut dyn DecisionProvider>,
    ) -> Result<(), EngineError> {
        match provider {
            Some(provider) => {
                let mut forwarded = LinkedReturnProvider { inner: provider };
                loop {
                    self.drain_linked_exile_returns(&mut forwarded)?;
                    self.check_state_based_actions_pass_with_provider(Some(&mut forwarded))?;
                    if !self.has_pending_linked_exile_returns() {
                        return Ok(());
                    }
                }
            }
            None => {
                self.check_state_based_actions_pass_with_provider(None)?;
                if self.has_pending_linked_exile_returns() {
                    return Err(EngineError::new(
                        "immediate linked return requires a decision provider",
                    ));
                }
                Ok(())
            }
        }
    }

    pub(super) fn resolve_combat_damage_step(
        &mut self,
        attacks: &BTreeMap<String, AttackAssignment>,
        blocks: &BTreeMap<String, Vec<String>>,
        assignments: &BTreeMap<String, BTreeMap<String, i32>>,
        step: CombatDamageStep,
    ) -> Result<(), EngineError> {
        self.resolve_combat_damage_step_with_provider(attacks, blocks, assignments, step, None)
    }
}

struct LinkedReturnProvider<'a> {
    inner: &'a mut dyn DecisionProvider,
}
impl DecisionProvider for LinkedReturnProvider<'_> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.inner.choose(state, request)
    }
    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        self.inner.choose_number(_state, request)
    }
    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        self.inner.choose_card_instance_ids(state, request)
    }
    fn choose_card_name(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        self.inner.choose_card_name(state, request)
    }
    fn requests_explicit_priority_pass(&self, _player_id: &str) -> bool {
        self.inner.requests_explicit_priority_pass(_player_id)
    }
    fn allows_combat_declaration_revisions(&self, _player_id: &str) -> bool {
        self.inner.allows_combat_declaration_revisions(_player_id)
    }
    fn observe_turn_completed(&mut self, _state: &GameState) {
        self.inner.observe_turn_completed(_state)
    }
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    fn begin_loop_shortcut(
        &mut self,
        _player_id: &str,
        _action_id: &str,
        _additional_iterations: usize,
    ) {
        self.inner
            .begin_loop_shortcut(_player_id, _action_id, _additional_iterations)
    }
    fn begin_loop_iteration(&mut self, _player_id: &str, _action_id: &str) {
        self.inner.begin_loop_iteration(_player_id, _action_id)
    }
    fn choose_loop_shortcut_policy(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.inner.choose_loop_shortcut_policy(_state, _request)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "linked_exile_return_tests.rs"]
mod tests;
