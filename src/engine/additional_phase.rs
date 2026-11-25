//! Phases added to the current turn (CR 500.8).
//!
//! `addCombatPhase` (`After this [main|combat] phase, there is an additional
//! combat phase [followed by an additional main phase]`) names the phase the
//! new phases follow and whether an additional main phase follows the added
//! combat phase. The phase anchor is resolved when the effect resolves:
//!
//! - the fieldless established form: the `additionalCombat` modifier, run
//!   after the turn's regular combat phase;
//! - a written anchor: an `additionalPhases` modifier consumed when the
//!   phase it follows ends (`run_phases_added_after_main_phase`,
//!   `run_phases_added_after_combat_phase`), including a phase that was
//!   itself added. An added main phase is a postcombat main phase (CR
//!   505.1a); phases added during an added phase follow it.
//!
//! An anchor that names a phase other than the current one (`after this main
//! phase` while no main phase is in progress) adds nothing (CR 500.8 names an
//! existing phase of this turn).
use super::*;

const MAX_ADDED_PHASES: usize = 64;

/// The phase an `addCombatPhase` effect names (`after`).
fn anchor_supported(anchor: &Value) -> bool {
    matches!(
        anchor.as_str(),
        Some("currentPhase" | "currentMainPhase" | "currentCombatPhase")
    )
}

/// `addCombatPhase` without fields is the established after-this-combat
/// form; its typed fields are the anchor and the following main phase.
pub(super) fn add_combat_phase_supported(effect: &Value) -> bool {
    effect.as_object().is_some_and(|fields| {
        fields
            .keys()
            .all(|key| ["kind", "after", "followedByMainPhase"].contains(&key.as_str()))
    }) && effect.get("after").is_none_or(anchor_supported)
        && effect
            .get("followedByMainPhase")
            .is_none_or(Value::is_boolean)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorPhase {
    Main,
    Combat,
}

impl AnchorPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Combat => "combat",
        }
    }
}

fn current_phase(step: &GameStep) -> Option<AnchorPhase> {
    match step {
        GameStep::PrecombatMain | GameStep::PostcombatMain => Some(AnchorPhase::Main),
        GameStep::BeginningOfCombat
        | GameStep::DeclareAttackers
        | GameStep::DeclareBlockers
        | GameStep::CombatDamage => Some(AnchorPhase::Combat),
        GameStep::Untap
        | GameStep::Upkeep
        | GameStep::Draw
        | GameStep::EndStep
        | GameStep::Cleanup => None,
    }
}

impl GameEngine {
    /// Resolves `addCombatPhase` with a written anchor (`after`): the added
    /// phases follow the phase the effect names, in the active player's turn.
    pub(super) fn schedule_added_combat_phase(&mut self, effect: &Value) {
        let Some(current) = current_phase(&self.state.step) else {
            return;
        };
        let anchor = match effect["after"].as_str() {
            Some("currentCombatPhase") => AnchorPhase::Combat,
            Some("currentMainPhase") => AnchorPhase::Main,
            _ => current,
        };
        if anchor != current {
            return;
        }
        let followed_by_main = effect["followedByMainPhase"].as_bool() == Some(true);
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        self.state.rule_modifiers.push(json!({
            "kind": "additionalPhases",
            "playerId": active_player_id,
            "turn": self.state.turn_number,
            "after": anchor.name(),
            "followedByMainPhase": followed_by_main,
        }));
    }

    /// Removes and returns the next phases added after the `anchor` phase
    /// now ending: whether an added main phase follows the added combat.
    /// CR 500.8: of several phases added after the same phase, the most
    /// recently created comes first.
    fn take_added_phases(&mut self, anchor: AnchorPhase) -> Option<bool> {
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        let index = self.state.rule_modifiers.iter().rposition(|modifier| {
            value_kind(modifier) == Some("additionalPhases")
                && modifier["playerId"].as_str() == Some(active_player_id.as_str())
                && modifier["turn"].as_u64() == Some(u64::from(self.state.turn_number))
                && modifier["after"].as_str() == Some(anchor.name())
        })?;
        let modifier = self.state.rule_modifiers.remove(index);
        Some(modifier["followedByMainPhase"].as_bool() == Some(true))
    }

    /// The phases added after the main phase now ending. Returns whether
    /// the turn ended during them.
    pub(super) fn run_phases_added_after_main_phase<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.run_added_phases(AnchorPhase::Main, provider, 0)
    }

    /// The phases added after the combat phase now ending. Returns whether
    /// the turn ended during them.
    pub(super) fn run_phases_added_after_combat_phase<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.run_added_phases(AnchorPhase::Combat, provider, 0)
    }

    fn run_added_phases<P: DecisionProvider>(
        &mut self,
        anchor: AnchorPhase,
        provider: &mut P,
        depth: usize,
    ) -> Result<bool, EngineError> {
        let mut added = 0;
        while let Some(followed_by_main) = self.take_added_phases(anchor) {
            added += 1;
            if added + depth > MAX_ADDED_PHASES {
                return Err(EngineError::new("too many additional phases"));
            }
            self.run_combat(provider)?;
            if self.finish_forced_turn(provider)? || self.state.outcome.is_some() {
                return Ok(true);
            }
            // Phases added after the added combat phase follow it.
            if self.run_added_phases(AnchorPhase::Combat, provider, depth + added)? {
                return Ok(true);
            }
            if !followed_by_main {
                continue;
            }
            // CR 505.1a: a main phase after a combat phase is a postcombat
            // main phase.
            self.transition_to_step(GameStep::PostcombatMain);
            self.enqueue_step_triggers("postcombatMain");
            self.run_priority_window(provider)?;
            if self.finish_forced_turn(provider)? || self.state.outcome.is_some() {
                return Ok(true);
            }
            if self.run_added_phases(AnchorPhase::Main, provider, depth + added)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
