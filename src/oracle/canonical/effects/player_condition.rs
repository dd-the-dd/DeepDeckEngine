//! Lowers one measure of a player's state, the operand of a condition
//! comparison (`Compare(Num, op, Num)` in `effects/condition.rs`).
//!
//! The measured player is a parameter: the controller (`controllerOf(self)`)
//! or, inside a `playerQuantifier`, the member it names with
//! `quantifiedPlayer`; the engine binds that reference to each member of the
//! population in turn (CR 102.2-102.3).
use super::super::*;

pub(in crate::oracle::canonical) fn lower_player_measure(
    measure: &PlayerStateMeasure,
    player: Value,
) -> Value {
    match measure {
        PlayerStateMeasure::ControlledPermanents {
            predicate,
            exclude_source,
        } => json!({
            "kind": "countPermanents",
            "player": player,
            "where": lower_permanent_object_predicate(predicate),
            "excludeSource": exclude_source,
        }),
        PlayerStateMeasure::Life => json!({ "kind": "lifeTotal", "player": player }),
        PlayerStateMeasure::PlayerCounters(counter) => json!({
            "kind": "countPlayerCounters",
            "player": player,
            "counter": counter.as_str(),
        }),
        PlayerStateMeasure::CardsInZone { zone, predicate } => {
            let zone_kind = match zone {
                PlayerCardZone::Hand => "hand",
                PlayerCardZone::Graveyard => "graveyard",
            };
            json!({
                "kind": "countCards",
                "zone": { "kind": zone_kind, "player": player },
                "where": lower_card_object_predicate(predicate),
            })
        }
        // The spells the player cast this turn are the player's spell-cast
        // events, filtered by the shared spell predicate when one is stated.
        PlayerStateMeasure::SpellsCastThisTurn(predicate) => json!({
            "kind": "countEventsThisTurn",
            "event": "spellCast",
            "player": player,
            "where": predicate
                .as_ref()
                .and_then(lower_spell_object_predicate)
                .unwrap_or(Value::Null),
        }),
        PlayerStateMeasure::CardsDrawnThisTurn => json!({
            "kind": "countEventsThisTurn",
            "event": "cardDrawn",
            "player": player,
            "where": Value::Null,
        }),
    }
}

#[cfg(test)]
#[path = "player_condition_tests.rs"]
mod tests;
