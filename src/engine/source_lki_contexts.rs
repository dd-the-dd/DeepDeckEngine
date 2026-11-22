//! Ephemeral source interpreters for immediately resolving abilities.
//! These frames are never gameplay stack objects. They subscribe to the same
//! actual-departure capture as pending stack abilities, so paying a sequence of
//! costs captures a source at its own departure, not before the entire payment.
use crate::engine::{CardInstance, EngineError, GameEngine, LegalAction, StackObject};

#[derive(Clone, Debug, Default)]
pub(in crate::engine) struct SourceLkiContexts {
    pub(super) frames: Vec<StackObject>,
    /// The departure capture of each permanent incarnation that left the
    /// battlefield through a central exit, keyed `instance@zoneIdentity`.
    /// A look-back trigger of that incarnation, published after it left
    /// (CR 603.10a), receives the same capture as a pending ability would.
    pub(super) departed: std::collections::BTreeMap<String, StackObject>,
}

impl GameEngine {
    /// The caller owns the ability actor and declared decisions. No identity is
    /// minted while reading a source, and nested scopes restore their own depth.
    pub(in crate::engine) fn with_immediate_source_context<T>(
        &mut self,
        action: &LegalAction,
        source: &CardInstance,
        work: impl FnOnce(&mut Self, usize) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let identity = Self::tracked_zone_identity(source).ok_or_else(|| {
            EngineError::new("immediate source context requires a tracked battlefield incarnation")
        })?;
        let live = self
            .permanent_position(&source.instance_id)
            .map(|(p, c)| &self.state.players[p].battlefield[c])
            .filter(|card| Self::tracked_zone_identity(card).as_ref() == Some(&identity))
            .ok_or_else(|| {
                EngineError::new("immediate source context is not the declared incarnation")
            })?;
        let frame = StackObject {
            id: format!("mana-ability:{}", action.id),
            controller: action.player_id.clone(),
            card: live.clone(),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("manaAbility".to_string()),
            ability_rule: None,
            decisions: action.decisions.clone(),
            targets: action.targets.clone(),
        };
        let depth = self.source_lki_contexts.frames.len();
        self.source_lki_contexts.frames.push(frame);
        let result = work(self, depth);
        // Full engine checkpoints clone these frames; partial cost checkpoints
        // restore them too. A nested failure must not leak an observer outward.
        self.source_lki_contexts.frames.truncate(depth);
        result
    }

    pub(in crate::engine) fn immediate_source_context(
        &self,
        index: usize,
    ) -> Result<StackObject, EngineError> {
        let frame = self
            .source_lki_contexts
            .frames
            .get(index)
            .ok_or_else(|| EngineError::new("immediate source context frame is absent"))?;
        if !super::source_has_departed(frame) {
            let live = self
                .permanent_position(&frame.card.instance_id)
                .map(|(p, c)| &self.state.players[p].battlefield[c]);
            if live.is_none_or(|card| {
                Self::tracked_zone_identity(card) != Self::tracked_zone_identity(&frame.card)
            }) {
                return Err(EngineError::new(
                    "immediate source departed without an authenticated LKI capture",
                ));
            }
        }
        Ok(frame.clone())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "source_lki_contexts_tests.rs"]
mod tests;
