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

/// Lowers the entire typed search contract for an explicitly resolved player.
/// Parent envelopes validate ownership; cardinality, reveal, binding and order
/// are properties of the search node and must survive every caller unchanged.
pub(in crate::oracle::canonical) fn lower_library_search_effects(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    player: Value,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.cardinality");
    cursor.require(
        search.minimum <= search.maximum,
        "a search minimum no greater than its maximum",
    )?;

    let binding = lower_collection_binding(search.binding);
    let mut effects = vec![json!({
        "kind": "chooseCards",
        "id": binding.clone(),
        "player": player.clone(),
        "minimum": i64::from(search.minimum),
        "maximum": i64::from(search.maximum),
        "candidates": {
            "kind": "cards",
            "zone": library(player.clone()),
            "where": lower_card_object_predicate(&search.filter),
        },
    })];
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
                    matches!(zone, CardZoneName::Hand | CardZoneName::Graveyard),
                    "a proved hand or graveyard search destination zone",
                )?;
                json!({"kind":zone.ir_name(), "player":player.clone()})
            },
            SearchDestination::LibraryTop => json!({
                "kind": "library",
                "player": player.clone(),
                "position": "top",
            }),
        },
    });
    let shuffle_effect = match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => json!({
            "kind": "shuffleZone",
            "zone": library(player.clone()),
        }),
        SearchShuffleOrder::ShuffleThenMove => json!({
            "kind": "shuffleZone",
            "zone": library(player.clone()),
            "excludeCards": decision_result(&binding),
        }),
    };
    match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => effects.extend([move_effect, shuffle_effect]),
        SearchShuffleOrder::ShuffleThenMove => effects.extend([shuffle_effect, move_effect]),
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
