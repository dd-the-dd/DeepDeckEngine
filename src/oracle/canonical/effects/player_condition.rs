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
        } => {
            // An ownership relation of the controller's own permanents (`you
            // control your commander`, CR 903.3) is the count's `ownership`
            // field, read against the counted player.
            if player == controller()
                && let Some((rest, Some(ownership))) = split_collection_ownership(predicate)
            {
                let mut count = lower_controlled_permanent_count(&rest, player, *exclude_source);
                if count["kind"] == "countPermanents" {
                    count["ownership"] = json!(ownership);
                    return count;
                }
                return lower_controlled_permanent_count(predicate, controller(), *exclude_source);
            }
            lower_controlled_permanent_count(predicate, player, *exclude_source)
        }
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
                PlayerCardZone::Library => "library",
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
        // CR 700.8: the engine's party-size quantity of that player.
        PlayerStateMeasure::PartySize => json!({ "kind": "partySize", "player": player }),
        // CR 700.5a: one count over the mana symbols of any listed color.
        PlayerStateMeasure::Devotion(colors) => json!({
            "kind": "devotionToColors",
            "player": player,
            "colors": colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
        }),
    }
}

#[cfg(test)]
#[path = "player_condition_tests.rs"]
mod tests;
