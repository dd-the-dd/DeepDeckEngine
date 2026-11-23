//! `countEvents` / `countEventsThisTurn`: the recorded events of one kind,
//! filtered by the player population the events relate to and by the type
//! of the object they recorded.
//!
//! The event log keeps an object's type line, not the object: a `where`
//! filter is read only for event kinds that record one, and only through
//! type atoms (card type, subtype, `not`, `and`, `or`). Any other predicate
//! is refused by the validator instead of being read as "every event". A
//! subtype is a phrase of the type line (`Time Lord`, `Assembly-Worker`).
//! A departure's creature status is its last-known one (`wasCreature`, CR
//! 603.10a: an animated land that dies is a creature that died).
//!
//! The `player` population relates to the event's player (the controller
//! of a departing permanent: Revolt's `a permanent you controlled left the
//! battlefield`), or, with `relation: "owner"`, to a departing card's owner
//! (`put into your graveyard`, CR 404.2). A population counts the events of
//! players who have since lost the game.
use super::*;

/// Event kinds whose detail records the object's `typeLine`.
const TYPED_EVENTS: &[&str] = &[
    "permanentDied",
    "permanentLeftBattlefield",
    "permanentEnteredBattlefield",
];

/// Departures, which record the card's owner.
const OWNED_EVENTS: &[&str] = &["permanentDied", "permanentLeftBattlefield"];

fn type_filter_supported(predicate: &Value) -> bool {
    match value_kind(predicate) {
        Some("cardTypeContains" | "subtypeContains") => predicate["value"]
            .as_str()
            .is_some_and(|value| !phrase_words(value).is_empty()),
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
    let relation_supported = match expression.get("relation") {
        None => true,
        Some(relation) => match relation.as_str() {
            Some("controller") => true,
            Some("owner") => OWNED_EVENTS.contains(&event) && expression.get("player").is_some(),
            _ => false,
        },
    };
    where_supported
        && relation_supported
        && expression
            .get("player")
            .is_none_or(player_query::population_supported)
        && expression.get("fromZone").is_none_or(Value::is_string)
}

/// Lowercased words of a type line or a type phrase; a dash between types
/// and subtypes is its own word, a hyphen inside a subtype is kept.
fn phrase_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| word.trim_matches(|character: char| character == ',' || character == '.'))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn type_line_contains_phrase(type_line: &str, phrase: &str) -> bool {
    let line = phrase_words(type_line);
    let wanted = phrase_words(phrase);
    !wanted.is_empty()
        && line
            .windows(wanted.len())
            .any(|window| window == wanted.as_slice())
}

fn type_filter_matches(detail: &Value, predicate: &Value) -> bool {
    let type_line = detail["typeLine"].as_str().unwrap_or_default();
    match value_kind(predicate) {
        // The last-known creature status, when the event records it.
        Some("cardTypeContains")
            if predicate["value"]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case("Creature"))
                && detail["wasCreature"].is_boolean() =>
        {
            detail["wasCreature"].as_bool() == Some(true)
        }
        Some("cardTypeContains" | "subtypeContains") => predicate["value"]
            .as_str()
            .is_some_and(|value| type_line_contains_phrase(type_line, value)),
        Some("not") => !type_filter_matches(detail, &predicate["operand"]),
        Some("and") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .all(|operand| type_filter_matches(detail, operand))
        }),
        Some("or") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .any(|operand| type_filter_matches(detail, operand))
        }),
        _ => false,
    }
}

impl GameEngine {
    /// The players an event count's `player` names, including players who
    /// have lost since (their recorded events still happened).
    pub(super) fn event_count_players(
        &self,
        context: player_query::PlayerQueryContext<'_>,
        reference: &Value,
    ) -> Option<Vec<String>> {
        let everyone = || self.state.players.iter().map(|player| player.id.clone());
        match value_kind(reference) {
            Some("eachPlayer") => Some(everyone().collect()),
            Some("opponentsOf" | "playersExcept") => {
                let except = context.resolve_ref(self, &reference["player"]).ok()?;
                Some(everyone().filter(|player| *player != except).collect())
            }
            _ => context.resolve_ref(self, reference).ok().map(|id| vec![id]),
        }
    }

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
        let owned = expression["relation"].as_str() == Some("owner");
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
                    let related = if owned {
                        event.detail["owner"].as_str()
                    } else {
                        event.player_id.as_deref()
                    };
                    related.is_some_and(|player| players.iter().any(|id| id == player))
                })
            })
            .filter(|event| {
                expression["fromZone"]
                    .as_str()
                    .is_none_or(|from_zone| event.detail["fromZone"].as_str() == Some(from_zone))
            })
            .filter(|event| {
                predicate.is_none_or(|predicate| type_filter_matches(&event.detail, predicate))
            })
            .count();
        i32::try_from(count).ok()
    }
}
