use super::super::*;

pub(in crate::oracle::canonical) fn lower_controller_library_search_effects(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.player");
    cursor.require(
        search.player == LocalPlayerReference::Controller,
        "the ability controller as the searching player",
    )?;
    lower_library_search_effects(cursor, search, controller())
}

/// The controller's search of a declared player's zones (`Search target
/// player's library ...`): `holder` is the lowered declared player, whose
/// zones are read; the controller chooses and the found cards move.
pub(in crate::oracle::canonical) fn lower_controller_search_of_holder(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    holder: Value,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.player");
    cursor.require(
        search.player == LocalPlayerReference::Controller,
        "the ability controller as the searching player",
    )?;
    lower_search_effects(cursor, search, controller(), holder)
}

/// Lowers the entire typed search contract for an explicitly resolved player.
/// Parent envelopes validate ownership; cardinality, reveal, binding and order
/// are properties of the search node and must survive every caller unchanged.
pub(in crate::oracle::canonical) fn lower_library_search_effects(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    player: Value,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.holder");
    cursor.require(
        search.origin.holder == SearchedPlayer::Searcher,
        "a search of the searching player's own zones; a declared holder is lowered by its program",
    )?;
    lower_search_effects(cursor, search, player.clone(), player)
}

/// `player` searches and receives the found cards; `holder` is the player
/// whose zones are searched and shuffled (CR 701.23a, 701.20a).
fn lower_search_effects(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    player: Value,
    holder: Value,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.cardinality");
    cursor.require(
        search.minimum <= search.maximum,
        "a search minimum no greater than its maximum",
    )?;

    let binding = lower_collection_binding(search.binding);
    let choice = if search.origin.is_library_only() {
        json!({
            "kind": "chooseCards",
            "id": binding.clone(),
            "player": player.clone(),
            "minimum": i64::from(search.minimum),
            "maximum": i64::from(search.maximum),
            "candidates": {
                "kind": "cards",
                "zone": library(holder.clone()),
                "where": lower_card_object_predicate(&search.filter),
            },
        })
    } else {
        // CR 701.23a: one choice over the union of the searched player's
        // zones; each zone is the shared card-zone query.
        cursor.enter("lower.library-search.origin-zones");
        let filter = lower_card_object_predicate(&search.filter);
        json!({
            "kind": "chooseCards",
            "id": binding.clone(),
            "player": player.clone(),
            "minimum": i64::from(search.minimum),
            "maximum": i64::from(search.maximum),
            "from": {
                "kind": "union",
                "sets": search.origin.zones.iter().map(|zone| json!({
                    "kind": "cardsInZone",
                    "zone": {"kind": zone.ir_name(), "player": holder.clone()},
                    "where": filter.clone(),
                })).collect::<Vec<_>>(),
            },
        })
    };
    let mut effects = vec![choice];
    if search.reveal {
        effects.push(json!({
            "kind": "revealCards",
            "cards": decision_result(&binding),
        }));
    }

    let move_effect = json!({
        "kind": "moveCards",
        "cards": decision_result(&binding),
        "to": match search.destination {
            SearchDestination::Battlefield { tapped } => json!({
                "kind": "battlefield",
                "player": player.clone(),
                "tapped": tapped,
            }),
            SearchDestination::Zone(zone) => {
                cursor.enter("lower.library-search.destination-zone");
                cursor.require(
                    matches!(
                        zone,
                        CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
                    ),
                    "a proved hand, graveyard or exile search destination zone",
                )?;
                if zone == CardZoneName::Exile {
                    // Exile is the cards' owners' (CR 406.2): the holder's.
                    json!({"kind":"exile", "player":holder.clone(), "faceDown":false})
                } else {
                    json!({"kind":zone.ir_name(), "player":player.clone()})
                }
            },
            SearchDestination::LibraryTop => json!({
                "kind": "library",
                "player": player.clone(),
                "position": "top",
            }),
        },
    });
    match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => effects.extend([
            move_effect,
            json!({
                "kind": "shuffleZone",
                "zone": library(holder.clone()),
            }),
        ]),
        SearchShuffleOrder::ShuffleThenMove => effects.extend([
            json!({
                "kind": "shuffleZone",
                "zone": library(holder.clone()),
                "excludeCards": decision_result(&binding),
            }),
            move_effect,
        ]),
        // The holder's own later instruction shuffles.
        SearchShuffleOrder::ByHolderInstruction => effects.push(move_effect),
    }
    effects.push(json!({
        "kind": "recordLibrarySearch",
        "player": player,
        "cards": decision_result(&binding),
    }));
    Ok(effects)
}

#[cfg(test)]
#[path = "library_search_move_tests.rs"]
mod tests;
