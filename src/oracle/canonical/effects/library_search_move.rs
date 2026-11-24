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
    let filter = lower_card_object_predicate(&search.filter);
    let move_effect = lower_search_move(cursor, search, &binding, &player, &holder)?;
    let found = |zones: &[CardZoneName]| {
        let mut effects = vec![search_choice(
            search, &binding, &player, &holder, zones, &filter,
        )];
        if search.reveal {
            effects.push(json!({
                "kind": "revealCards",
                "cards": decision_result(&binding),
            }));
        }
        effects
    };
    let record = json!({
        "kind": "recordLibrarySearch",
        "player": player.clone(),
        "cards": decision_result(&binding),
    });
    let shuffle = json!({
        "kind": "shuffleZone",
        "zone": library(holder.clone()),
    });

    let mut effects = match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => {
            let mut effects = found(&search.origin.zones);
            effects.extend([move_effect, shuffle]);
            effects
        }
        SearchShuffleOrder::ShuffleThenMove => {
            let mut effects = found(&search.origin.zones);
            effects.extend([
                json!({
                    "kind": "shuffleZone",
                    "zone": library(holder.clone()),
                    "excludeCards": decision_result(&binding),
                }),
                move_effect,
            ]);
            effects
        }
        // The holder's own later instruction shuffles.
        SearchShuffleOrder::ByHolderInstruction => {
            let mut effects = found(&search.origin.zones);
            effects.push(move_effect);
            effects
        }
        // CR 701.23a: the searcher chooses the zones of `library and/or
        // graveyard`. Searching more zones never narrows the choice, so the
        // decision that matters is whether the library is searched: then the
        // choice reads every listed zone and the library is shuffled;
        // otherwise it reads the other zones and nothing is shuffled.
        SearchShuffleOrder::IfLibrarySearched => {
            cursor.enter("lower.library-search.conditional-shuffle");
            let other_zones = search
                .origin
                .zones
                .iter()
                .copied()
                .filter(|zone| *zone != CardZoneName::Library)
                .collect::<Vec<_>>();
            cursor.require(
                !other_zones.is_empty() && other_zones.len() < search.origin.zones.len(),
                "a search of the library and at least one other zone",
            )?;
            let mut with_library = found(&search.origin.zones);
            with_library.extend([move_effect.clone(), shuffle, record]);
            let mut without_library = found(&other_zones);
            without_library.push(move_effect);
            return Ok(vec![json!({
                "kind": "chooseOneEffect",
                "player": player,
                "options": [
                    {"label": "Search your library", "effects": with_library},
                    {"label": "Don't search your library", "effects": without_library},
                ],
            })]);
        }
    };
    effects.push(record);
    Ok(effects)
}

/// One choice of the found cards among the searched zones (CR 701.23a): the
/// library alone keeps its candidate set; several zones are one union of the
/// shared card-zone query; one other zone is that zone's query.
fn search_choice(
    search: &LibrarySearchEffectNode,
    binding: &str,
    player: &Value,
    holder: &Value,
    zones: &[CardZoneName],
    filter: &Value,
) -> Value {
    let mut choice = json!({
        "kind": "chooseCards",
        "id": binding,
        "player": player.clone(),
        "minimum": i64::from(search.minimum),
        "maximum": i64::from(search.maximum),
    });
    let zone_query = |zone: &CardZoneName| {
        json!({
            "kind": "cardsInZone",
            "zone": {"kind": zone.ir_name(), "player": holder.clone()},
            "where": filter.clone(),
        })
    };
    match zones {
        [CardZoneName::Library] => {
            choice["candidates"] = json!({
                "kind": "cards",
                "zone": library(holder.clone()),
                "where": filter.clone(),
            });
        }
        [zone] => choice["from"] = zone_query(zone),
        zones => {
            choice["from"] = json!({
                "kind": "union",
                "sets": zones.iter().map(zone_query).collect::<Vec<_>>(),
            });
        }
    }
    choice
}

/// The move of the found cards to the search destination.
fn lower_search_move(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    binding: &str,
    player: &Value,
    holder: &Value,
) -> MachineResult<Value> {
    Ok(json!({
        "kind": "moveCards",
        "cards": decision_result(binding),
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
    }))
}

#[cfg(test)]
#[path = "library_search_move_tests.rs"]
mod tests;
