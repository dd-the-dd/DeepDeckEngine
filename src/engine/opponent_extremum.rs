//! `opponentExtremum`: the greatest or least value of one player measure
//! among the opponents of the ability's controller (CR 102.2-102.3).
//!
//! ```text
//! { kind: opponentExtremum, extremum: greatest | least,
//!   player: <the ability's controller>, value: <measure of quantifiedPlayer> }
//! ```
//!
//! `an opponent has more cards in hand than you` holds iff the greatest
//! opponent hand exceeds the controller's; `you have more life than each
//! opponent` iff the controller's life exceeds the greatest opponent life.
//! The measure is one proposition about a single player written with the
//! `quantifiedPlayer` reference; it is evaluated with each living opponent as
//! its reference player. A measure naming any other player is refused, and a
//! game without a living opponent has no extremum (`None`), never zero.
use super::*;

const QUANTIFIED_PLAYER: &str = "quantifiedPlayer";
const MAX_DEPTH: usize = 64;

/// Player references other than the quantified one: inside the measure they
/// would be read against the bound opponent.
const OTHER_PLAYER_REFERENCES: &[&str] = &[
    "controller",
    "abilityController",
    "controllerOf",
    "controllerOfObject",
    "controllerOfAttachedPermanent",
    "ownerOf",
    "triggeringPlayer",
    "opponentsOf",
    "eachPlayer",
    "playersExcept",
];

fn names_only_the_quantified_player(value: &Value, depth: usize) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| names_only_the_quantified_player(value, depth + 1)),
        Value::Object(object) => {
            !value_kind(value).is_some_and(|kind| OTHER_PLAYER_REFERENCES.contains(&kind))
                && object
                    .values()
                    .all(|value| names_only_the_quantified_player(value, depth + 1))
        }
        _ => true,
    }
}

/// The measure with `quantifiedPlayer` read as the evaluation's reference
/// player, which is then bound to one opponent.
fn bind_quantified_player(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(bind_quantified_player).collect()),
        Value::Object(object) => {
            if value_kind(value) == Some(QUANTIFIED_PLAYER) {
                return json!({ "kind": "controllerOf", "object": { "kind": "self" } });
            }
            Value::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), bind_quantified_player(value)))
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

/// The anchor is the ability's controller: its opponents are the population.
fn anchored_on_controller(player: &Value) -> bool {
    value_kind(player) == Some("abilityController")
        || (value_kind(player) == Some("controllerOf")
            && matches!(
                value_kind(&player["object"]),
                Some("self" | "abilitySource")
            ))
}

pub(super) fn opponent_extremum_supported(expression: &Value) -> bool {
    matches!(expression["extremum"].as_str(), Some("greatest" | "least"))
        && anchored_on_controller(&expression["player"])
        && expression["value"].is_object()
        && names_only_the_quantified_player(&expression["value"], 0)
        && numeric_expression_supported(&bind_quantified_player(&expression["value"]))
}

impl GameEngine {
    /// The extremum over the living opponents of `controller_id`.
    pub(super) fn opponent_extremum(&self, expression: &Value, controller_id: &str) -> Option<i32> {
        if !opponent_extremum_supported(expression) {
            return None;
        }
        let measure = bind_quantified_player(&expression["value"]);
        let values = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost && player.id != controller_id)
            .map(|player| self.numeric_expression_with_source(&measure, &player.id, None))
            .collect::<Option<Vec<_>>>()?;
        match expression["extremum"].as_str() {
            Some("greatest") => values.into_iter().max(),
            Some("least") => values.into_iter().min(),
            _ => None,
        }
    }
}
