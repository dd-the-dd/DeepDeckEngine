//! `countEventsPreviousTurn`: the number of recorded events of one kind that
//! occurred during the turn immediately before the current one (the
//! `last turn` of Oracle text, e.g. the werewolf transform conditions).
//!
//! `aggregate: "total"` counts every matching event of every player ("no
//! spells were cast last turn"); `aggregate: "greatestPerPlayer"` counts each
//! player's events separately and yields the largest tally ("a player cast two
//! or more spells last turn"). The first turn has no previous turn: the count
//! is zero.
use super::*;

/// Shape accepted by the executable engine: a named event and a known
/// aggregate. Any other aggregate is refused rather than defaulted.
pub(super) fn previous_turn_event_count_supported(expression: &Value) -> bool {
    expression["event"].is_string()
        && matches!(
            expression["aggregate"].as_str(),
            Some("total" | "greatestPerPlayer")
        )
}

impl GameEngine {
    pub(super) fn previous_turn_event_count(&self, expression: &Value) -> Option<i32> {
        let event_kind = expression["event"].as_str()?;
        let aggregate = expression["aggregate"].as_str()?;
        let Some(previous_turn) = self.state.turn_number.checked_sub(1) else {
            return Some(0);
        };
        let mut per_player = BTreeMap::<&str, i32>::new();
        let mut total = 0;
        for event in self
            .state
            .events
            .iter()
            .filter(|event| event.turn_number == previous_turn && event.kind == event_kind)
        {
            total += 1;
            *per_player
                .entry(event.player_id.as_deref().unwrap_or(""))
                .or_default() += 1;
        }
        match aggregate {
            "total" => Some(total),
            "greatestPerPlayer" => Some(per_player.values().copied().max().unwrap_or(0)),
            _ => None,
        }
    }
}
