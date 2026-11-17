use super::GameEvent;

pub(super) fn count_turns_taken(events: &[GameEvent], player_id: &str) -> i32 {
    events
        .iter()
        .filter(|event| event.kind == "turnStarted")
        .filter(|event| event.player_id.as_deref() == Some(player_id))
        .count()
        .min(i32::MAX as usize) as i32
}
