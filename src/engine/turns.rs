use super::GameEvent;

pub(super) fn count_turns_taken(events: &[GameEvent], player_id: &str) -> i32 {
    events
        .iter()
        .filter(|event| event.kind == "turnStarted")
        .filter(|event| event.player_id.as_deref() == Some(player_id))
        .count()
        .min(i32::MAX as usize) as i32
}

/// The rules-state kind of a scheduled skip (CR 500.11, 614.10): the next
/// `remaining` occurrences of `part` for `playerId` do not happen.
const SKIPPED_TURN_PART: &str = "skippedTurnPart";

impl super::GameEngine {
    /// Schedules `count` skips of `part` (`turn`, `untapStep`, `drawStep`,
    /// `combatPhase`) for the player. `this_turn` limits the skip to an
    /// occurrence in the current turn.
    pub(super) fn schedule_skipped_turn_part(
        &mut self,
        player_id: &str,
        part: &str,
        count: u64,
        this_turn: bool,
    ) {
        if count == 0 {
            return;
        }
        self.state.rule_modifiers.push(serde_json::json!({
            "kind": SKIPPED_TURN_PART,
            "playerId": player_id,
            "part": part,
            "remaining": count,
            "installedTurn": self.state.turn_number,
            "thisTurnOnly": this_turn,
        }));
    }

    /// Consumes one scheduled skip of the player's `part`, if any, and
    /// records it. A `this turn` skip scheduled in an earlier turn has
    /// expired and is discarded.
    pub(super) fn consume_skipped_turn_part(&mut self, player_id: &str, part: &str) -> bool {
        let turn = u64::from(self.state.turn_number);
        self.state.rule_modifiers.retain(|modifier| {
            !(super::value_kind(modifier) == Some(SKIPPED_TURN_PART)
                && modifier["thisTurnOnly"].as_bool() == Some(true)
                && modifier["installedTurn"].as_u64() != Some(turn))
        });
        let Some(index) = self.state.rule_modifiers.iter().position(|modifier| {
            super::value_kind(modifier) == Some(SKIPPED_TURN_PART)
                && modifier["playerId"].as_str() == Some(player_id)
                && modifier["part"].as_str() == Some(part)
        }) else {
            return false;
        };
        let remaining = self.state.rule_modifiers[index]["remaining"]
            .as_u64()
            .unwrap_or(1);
        if remaining <= 1 {
            self.state.rule_modifiers.remove(index);
        } else {
            self.state.rule_modifiers[index]["remaining"] = serde_json::json!(remaining - 1);
        }
        self.record_event(
            "turnPartSkipped",
            Some(player_id.to_string()),
            None,
            serde_json::json!({ "part": part }),
        );
        true
    }

    /// The active player's skipped untap step (CR 502): none of their
    /// permanents untaps this turn.
    pub(super) fn skip_untap_step_if_scheduled(
        &mut self,
        active: usize,
        remain_tapped: &mut std::collections::BTreeSet<String>,
    ) {
        let player_id = self.state.players[active].id.clone();
        if self.consume_skipped_turn_part(&player_id, "untapStep") {
            remain_tapped.extend(
                self.state.players[active]
                    .battlefield
                    .iter()
                    .map(|permanent| permanent.instance_id.clone()),
            );
        }
    }

    /// The active player's skipped combat phase (CR 506): the phase does not
    /// begin.
    pub(super) fn skip_combat_phase_if_scheduled(&mut self) -> bool {
        let player_id = self.state.players[self.state.active_player].id.clone();
        self.consume_skipped_turn_part(&player_id, "combatPhase")
    }
}
