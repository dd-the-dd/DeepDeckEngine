//! `countEvents` / `countEventsThisTurn`: the recorded events of one kind,
//! filtered by the player population that performed them and by the type of
//! the object they recorded.
//!
//! The event log keeps an object's type line, not the object: a `where`
//! filter is read only for event kinds that record one, and only through
//! type atoms (card type, subtype, `not`, `and`, `or`). Any other predicate
//! is refused by the validator instead of being read as "every event".
use super::*;

/// Event kinds whose detail records the object's `typeLine`.
const TYPED_EVENTS: &[&str] = &[
    "permanentDied",
    "permanentLeftBattlefield",
    "permanentEnteredBattlefield",
];

fn type_filter_supported(predicate: &Value) -> bool {
    match value_kind(predicate) {
        Some("cardTypeContains" | "subtypeContains") => predicate["value"].is_string(),
        Some("not") => type_filter_supported(&predicate["operand"]),
        Some("and" | "or") => predicate["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(type_filter_supported)
        }),
        _ => false,
    }
}

/// The validator's reading of an event count's shape.
pub(super) fn event_count_supported(expression: &Value) -> bool {
    let Some(event) = expression["event"].as_str() else {
        return false;
    };
    let where_supported = match expression.get("where") {
        None | Some(Value::Null) => true,
        Some(predicate) => TYPED_EVENTS.contains(&event) && type_filter_supported(predicate),
    };
    where_supported
        && expression
            .get("player")
            .is_none_or(player_query::population_supported)
        && expression.get("fromZone").is_none_or(Value::is_string)
}

fn type_line_words(type_line: &str) -> Vec<String> {
    type_line
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn type_filter_matches(type_line: &str, predicate: &Value) -> bool {
    match value_kind(predicate) {
        Some("cardTypeContains" | "subtypeContains") => {
            predicate["value"].as_str().is_some_and(|wanted| {
                let wanted = wanted.to_ascii_lowercase();
                type_line_words(type_line).contains(&wanted)
            })
        }
        Some("not") => !type_filter_matches(type_line, &predicate["operand"]),
        Some("and") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .all(|operand| type_filter_matches(type_line, operand))
        }),
        Some("or") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .any(|operand| type_filter_matches(type_line, operand))
        }),
        _ => false,
    }
}

impl GameEngine {
    /// Counts the matching events. `players` is the resolved population of
    /// the expression's `player` (absent: every player's events count).
    pub(super) fn count_matching_events(
        &self,
        expression: &Value,
        players: Option<&[String]>,
    ) -> Option<i32> {
        if !event_count_supported(expression) {
            return None;
        }
        let event_kind = expression["event"].as_str()?;
        let current_turn_only = value_kind(expression) == Some("countEventsThisTurn");
        let predicate = expression
            .get("where")
            .filter(|predicate| !predicate.is_null());
        let count = self
            .state
            .events
            .iter()
            .filter(|event| {
                (!current_turn_only || event.turn_number == self.state.turn_number)
                    && event.kind == event_kind
            })
            .filter(|event| {
                players.is_none_or(|players| {
                    event
                        .player_id
                        .as_ref()
                        .is_some_and(|player| players.contains(player))
                })
            })
            .filter(|event| {
                expression["fromZone"]
                    .as_str()
                    .is_none_or(|from_zone| event.detail["fromZone"].as_str() == Some(from_zone))
            })
            .filter(|event| {
                predicate.is_none_or(|predicate| {
                    event.detail["typeLine"]
                        .as_str()
                        .is_some_and(|type_line| type_filter_matches(type_line, predicate))
                })
            })
            .count();
        i32::try_from(count).ok()
    }
}
