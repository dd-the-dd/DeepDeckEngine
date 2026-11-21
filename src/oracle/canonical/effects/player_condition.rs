//! Lowers a player-state proposition and its player subject to condition IR.
//!
//! The proposition is one comparison over a measure of a single player. A
//! controller subject measures `controllerOf(self)`. A quantified subject
//! wraps the same comparison in `playerQuantifier`, whose inner condition
//! names the quantified player with `quantifiedPlayer`; the engine binds that
//! reference to each member of the population in turn (CR 102.2-102.3).
use super::super::*;

fn quantified_player() -> Value {
    json!({ "kind": "quantifiedPlayer" })
}

fn threshold_operator(threshold: CountThreshold) -> &'static str {
    match threshold.operator {
        ThresholdOperator::AtLeast => ">=",
        ThresholdOperator::AtMost => "<=",
    }
}

fn lower_player_state_proposition(proposition: &PlayerStateProposition, player: Value) -> Value {
    let threshold = proposition.threshold;
    let measured = |measure: Value| {
        compare(
            threshold_operator(threshold),
            measure,
            integer(threshold.value),
        )
    };
    match &proposition.measure {
        PlayerStateMeasure::ControlledPermanents(predicate) => measured(json!({
            "kind": "countPermanents",
            "player": player,
            "where": lower_permanent_object_predicate(predicate),
            "excludeSource": false,
        })),
        PlayerStateMeasure::Life => measured(json!({ "kind": "lifeTotal", "player": player })),
        PlayerStateMeasure::PlayerCounters(counter) => measured(json!({
            "kind": "countPlayerCounters",
            "player": player,
            "counter": counter.as_str(),
        })),
        PlayerStateMeasure::CardsInZone { zone, predicate } => {
            let zone_kind = match zone {
                PlayerCardZone::Hand => "hand",
                PlayerCardZone::Graveyard => "graveyard",
            };
            measured(json!({
                "kind": "countCards",
                "zone": { "kind": zone_kind, "player": player },
                "where": lower_card_object_predicate(predicate),
            }))
        }
        // A filtered presence reads the spells the player cast this turn
        // through the shared card filter; the grammar admits no filtered count.
        PlayerStateMeasure::SpellsCastThisTurn(Some(predicate)) => json!({
            "kind": "hasCastSpellThisTurn",
            "player": player,
            "where": lower_spell_object_predicate(predicate).unwrap_or(Value::Null),
        }),
        PlayerStateMeasure::SpellsCastThisTurn(None) => measured(json!({
            "kind": "countEventsThisTurn",
            "event": "spellCast",
            "player": player,
            "where": Value::Null,
        })),
        PlayerStateMeasure::CardsDrawnThisTurn => measured(json!({
            "kind": "countEventsThisTurn",
            "event": "cardDrawn",
            "player": player,
            "where": Value::Null,
        })),
    }
}

pub(in crate::oracle::canonical) fn lower_player_state_condition(
    node: &PlayerStateConditionNode,
) -> Value {
    match node.subject {
        ConditionPlayerSubject::Controller => {
            lower_player_state_proposition(&node.proposition, controller())
        }
        ConditionPlayerSubject::OpponentsTogether => {
            let PlayerStateMeasure::ControlledPermanents(predicate) = &node.proposition.measure
            else {
                unreachable!("the collective subject reads only a controlled-permanent count")
            };
            let threshold = node.proposition.threshold;
            compare(
                threshold_operator(threshold),
                json!({
                    "kind": "countPermanentsControlledByOpponents",
                    "player": controller(),
                    "where": lower_permanent_object_predicate(predicate),
                }),
                integer(threshold.value),
            )
        }
        ConditionPlayerSubject::Quantified {
            quantifier,
            population,
        } => {
            let players = match population {
                PlayerPopulation::Opponents => {
                    json!({ "kind": "opponentsOf", "player": controller() })
                }
                PlayerPopulation::Players => json!({ "kind": "eachPlayer" }),
            };
            let quantified = |quantifier: &str| {
                json!({
                    "kind": "playerQuantifier",
                    "quantifier": quantifier,
                    "players": players,
                    "condition": lower_player_state_proposition(&node.proposition, quantified_player()),
                })
            };
            match quantifier {
                PlayerQuantifier::Some => quantified("some"),
                PlayerQuantifier::Every => quantified("every"),
                // `no opponent ...` is the negation of `an opponent ...`.
                PlayerQuantifier::None => not(quantified("some")),
            }
        }
    }
}

#[cfg(test)]
#[path = "player_condition_tests.rs"]
mod tests;
