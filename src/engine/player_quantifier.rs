//! `playerQuantifier`: a condition quantified over a player population
//! (CR 102.2-102.3: `an opponent ...`, `each player ...`).
//!
//! The inner condition is one proposition about a single player, written with
//! the `quantifiedPlayer` reference. It is evaluated once per living member of
//! the population with that member as the proposition's reference player, so
//! every player-parameterized measure the shared evaluator already supports
//! (permanent and card counts, life total, player counters, this-turn history)
//! composes without a quantifier-specific measure. Because the member becomes
//! the reference player, the inner condition must not name any other player;
//! such a mixed proposition is refused rather than evaluated against the wrong
//! player.
use super::*;

const QUANTIFIED_PLAYER: &str = "quantifiedPlayer";
const MAX_DEPTH: usize = 64;

/// Player references other than the quantified one. Their presence inside the
/// quantified proposition would be resolved against the bound member.
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quantifier {
    Some,
    Every,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Population {
    Opponents,
    Players,
}

fn quantifier(condition: &Value) -> Option<Quantifier> {
    match condition["quantifier"].as_str() {
        Some("some") => Some(Quantifier::Some),
        Some("every") => Some(Quantifier::Every),
        _ => None,
    }
}

/// The population relative to the ability controller: its opponents, or every
/// player. Another anchor player has no evaluator here.
fn population(players: &Value) -> Option<Population> {
    match value_kind(players) {
        Some("eachPlayer") => Some(Population::Players),
        Some("opponentsOf") => {
            let anchor = &players["player"];
            let controller = value_kind(anchor) == Some("abilityController")
                || (value_kind(anchor) == Some("controllerOf")
                    && matches!(
                        value_kind(&anchor["object"]),
                        Some("self" | "abilitySource")
                    ));
            controller.then_some(Population::Opponents)
        }
        _ => None,
    }
}

pub(super) fn names_only_the_quantified_player(value: &Value, depth: usize) -> bool {
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

/// Rewrites `quantifiedPlayer` as the reference player of the evaluation
/// context, which is then bound to one population member.
pub(super) fn bind_quantified_player(value: &Value) -> Value {
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

pub(super) fn player_quantifier_supported(condition: &Value) -> bool {
    let inner = &condition["condition"];
    quantifier(condition).is_some()
        && population(&condition["players"]).is_some()
        && inner.is_object()
        && names_only_the_quantified_player(inner, 0)
        && condition_supported(&bind_quantified_player(inner))
}

impl GameEngine {
    pub(super) fn player_quantifier_holds(
        &self,
        condition: &Value,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        source: Option<&CardInstance>,
    ) -> bool {
        let (Some(quantifier), Some(population)) =
            (quantifier(condition), population(&condition["players"]))
        else {
            return false;
        };
        let inner = bind_quantified_player(&condition["condition"]);
        let mut members = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .filter(|player| population == Population::Players || player.id != controller_id)
            .map(|player| player.id.as_str());
        let member_holds =
            |member: &str| self.condition_value_with_source(&inner, member, decisions, source);
        match quantifier {
            Quantifier::Some => members.any(member_holds),
            Quantifier::Every => members.all(member_holds),
        }
    }
}
