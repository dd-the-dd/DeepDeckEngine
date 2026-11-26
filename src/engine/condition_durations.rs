//! Continuous effects bounded by a watched state (CR 611.2b).
//!
//! `for as long as SOURCE remains tapped`, `for as long as you control
//! SOURCE` and `for as long as SOURCE remains on the battlefield` are
//! durations of the ordinary effect primitives (a power/toughness change, a
//! keyword grant), not separate operations. When such an effect starts, the
//! primitive asks this module for the *watch* of its duration and stores it
//! on the rule modifier it creates. The watch names the same source object
//! (CR 400.7: a new incarnation is a new object) and the state it must keep:
//!
//! - the effect never starts when the state does not hold as it would
//!   begin (CR 611.2b);
//! - a modifier whose watch no longer holds is ignored by its readers and
//!   removed at the next state check, so an effect that ended stays ended
//!   even if the state holds again later.
//!
//! The turn-structure durations (`untilEndOfCurrentTurn`, `untilNextTurn`)
//! stay with their primitives; this module only owns watched states.
use super::*;

/// The rule-modifier field holding the watch of a state-bounded duration.
pub(super) const WATCH_FIELD: &str = "watch";

/// The `expiresBeforeTurn` of a modifier that only its watch ends.
pub(super) const WATCHED_EXPIRY_TURN: u32 = u32::MAX;

/// The source states a duration can watch.
fn watched_state(duration: &Value) -> Option<&'static str> {
    match value_kind(duration) {
        Some("whileSourceTapped") => Some("tapped"),
        Some("whileSourceOnBattlefield") => Some("onBattlefield"),
        Some("whileSourceControlled") => Some("controlled"),
        _ => None,
    }
}

/// Whether `duration` is a state-bounded duration this module can watch.
pub(super) fn watched_duration_supported(duration: &Value) -> bool {
    watched_state(duration).is_some()
}

/// How an effect with a duration starts.
pub(super) enum WatchedDurationStart {
    /// Not a watched duration: the primitive keeps its own lifetime.
    Unwatched,
    /// A watched duration whose state does not hold now (CR 611.2b): the
    /// effect does nothing.
    NeverStarts,
    /// A watched duration in force: store this watch on the modifier.
    Watch(Value),
}

impl GameEngine {
    /// The watch of `duration` for an effect created by `stack_object`.
    pub(super) fn begin_watched_duration(
        &self,
        duration: &Value,
        stack_object: &StackObject,
    ) -> WatchedDurationStart {
        let Some(state) = watched_state(duration) else {
            return WatchedDurationStart::Unwatched;
        };
        let watch = json!({
            "state": state,
            "sourceCardInstanceId": stack_object.card.instance_id,
            "sourceIdentity": Self::tracked_zone_identity(&stack_object.card),
            "controllerId": stack_object.controller,
        });
        if self.watch_holds(&watch) {
            WatchedDurationStart::Watch(watch)
        } else {
            WatchedDurationStart::NeverStarts
        }
    }

    /// Whether the watch of `watch` still holds: the same source object is
    /// on the battlefield and keeps the watched state.
    fn watch_holds(&self, watch: &Value) -> bool {
        let Some(source_id) = watch["sourceCardInstanceId"].as_str() else {
            return false;
        };
        let Some((player, index)) = self.permanent_position(source_id) else {
            return false;
        };
        let source = &self.state.players[player].battlefield[index];
        if Self::tracked_zone_identity(source) != watch["sourceIdentity"].as_str() {
            return false;
        }
        match watch["state"].as_str() {
            Some("tapped") => source.tapped,
            Some("onBattlefield") => true,
            Some("controlled") => {
                watch["controllerId"].as_str() == Some(source.controller.as_str())
            }
            _ => false,
        }
    }

    /// Whether a rule modifier is still in force as far as its watched
    /// duration is concerned. A modifier without a watch is unaffected.
    pub(super) fn watched_duration_holds(&self, modifier: &Value) -> bool {
        modifier
            .get(WATCH_FIELD)
            .is_none_or(|watch| self.watch_holds(watch))
    }

    /// Ends every watched effect whose state no longer holds (CR 611.2b).
    /// Called with the state checks, so an effect that ended is gone before
    /// its state could hold again.
    pub(super) fn expire_watched_durations(&mut self) {
        let expired = self
            .state
            .rule_modifiers
            .iter()
            .map(|modifier| !self.watched_duration_holds(modifier))
            .collect::<Vec<_>>();
        if !expired.contains(&true) {
            return;
        }
        let mut index = 0;
        self.state.rule_modifiers.retain(|_| {
            let keep = !expired[index];
            index += 1;
            keep
        });
    }
}
