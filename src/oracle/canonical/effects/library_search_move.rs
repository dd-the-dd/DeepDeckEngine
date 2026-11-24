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
    // A typed count is read by the one choice of the found cards; a slot
    // search always has one literal card per slot.
    cursor.require(
        search.count.is_none() || search.slots.is_empty(),
        "a typed search count over one card phrase",
    )?;
    let typed_bounds =
        match &search.count {
            Some(count) => Some(lower_search_count_bounds(count).ok_or_else(|| {
                cursor.fail("a search count the shared numeric lowering represents")
            })?),
            None => None,
        };

    let binding = lower_collection_binding(search.binding);
    let filter = lower_card_object_predicate(&search.filter);
    // One choice, or one choice per slot (`a Forest card and a Plains
    // card`): the found cards are that choice's result or the slots' union.
    let slot_ids = (0..search.slots.len())
        .map(|index| format!("{binding}/slot{index}"))
        .collect::<Vec<_>>();
    let found_cards = search_found_cards(search);
    let found = |zones: &[CardZoneName]| {
        let mut effects = if search.slots.is_empty() {
            let mut choice = search_choice(search, &binding, &player, &holder, zones, &filter);
            if let Some((minimum, maximum)) = &typed_bounds {
                choice["minimum"] = minimum.clone();
                // `any number of` (CR 701.23a): every matching card of the
                // searched zones may be found; their count is the maximum.
                choice["maximum"] = maximum.clone().unwrap_or_else(|| {
                    json!({
                        "kind": "countObjects",
                        "objects": searched_zone_cards(&holder, zones, &filter),
                    })
                });
            }
            vec![choice]
        } else {
            search_slot_choices(search, &slot_ids, &player, &holder, zones)
        };
        if search.reveal {
            effects.push(json!({
                "kind": "revealCards",
                "cards": found_cards.clone(),
            }));
        }
        effects
    };
    let record = json!({
        "kind": "recordLibrarySearch",
        "player": player.clone(),
        "cards": found_cards.clone(),
    });
    // The search ends at its selection: later ordinary actions of the
    // program reveal, move and shuffle (the shuffle is never inserted here).
    if search.destination == SearchDestination::Bound {
        cursor.enter("lower.library-search.bound-result");
        cursor.require(
            !search.reveal && search.shuffle_order == SearchShuffleOrder::ByHolderInstruction,
            "a bound search whose later actions reveal and shuffle",
        )?;
        let mut effects = found(&search.origin.zones);
        effects.push(record);
        return Ok(effects);
    }
    let move_effect = lower_search_move(cursor, search, found_cards.clone(), &player, &holder)?;
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
                    "excludeCards": found_cards.clone(),
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

/// The bounds of a typed search count (`up to X`, `that many`, `any number
/// of`): the shared target-count amounts. An unbounded maximum is `None`;
/// the choice then counts the matching cards of the searched zones.
fn lower_search_count_bounds(count: &TargetCount) -> Option<(Value, Option<Value>)> {
    let maximum = match &count.maximum {
        TargetCountBound::Amount(maximum) => Some(lower_target_count_amount(maximum)?),
        TargetCountBound::Unbounded => None,
    };
    Some((lower_target_count_amount(&count.minimum)?, maximum))
}

/// Every card of the searched zones that matches the search's card phrase:
/// the population an unbounded search may find.
fn searched_zone_cards(holder: &Value, zones: &[CardZoneName], filter: &Value) -> Value {
    let zone_cards = |zone: &CardZoneName| {
        let mut cards = json!({
            "kind": "cardsInZone",
            "zone": {"kind": zone.ir_name(), "player": holder.clone()},
        });
        if !filter.is_null() {
            cards["where"] = filter.clone();
        }
        cards
    };
    match zones {
        [zone] => zone_cards(zone),
        zones => json!({
            "kind": "union",
            "sets": zones.iter().map(zone_cards).collect::<Vec<_>>(),
        }),
    }
}

/// The cards one search found: its choice's result, or the union of its
/// slot choices (`a Forest card and a Plains card`).
fn search_found_cards(search: &LibrarySearchEffectNode) -> Value {
    let binding = lower_collection_binding(search.binding);
    if search.slots.is_empty() {
        decision_result(&binding)
    } else {
        json!({
            "kind": "union",
            "sets": (0..search.slots.len())
                .map(|index| decision_result(&format!("{binding}/slot{index}")))
                .collect::<Vec<_>>(),
        })
    }
}

/// The static upper bound on how many cards a search finds, when its count
/// is literal: one card per slot, or the literal maximum.
fn search_found_maximum(search: &LibrarySearchEffectNode) -> Option<i64> {
    if !search.slots.is_empty() {
        return i64::try_from(search.slots.len()).ok();
    }
    match &search.count {
        None => Some(i64::from(search.maximum)),
        Some(count) => match &count.maximum {
            TargetCountBound::Amount(NumericExpression::Atom(NumericAtom::Integer(value))) => {
                Some(*value)
            }
            _ => None,
        },
    }
}

/// After a search that ends at its selection, its found cards become the
/// observed card collection of the program (CR 608.2c): `it`, `that card`,
/// `them`, `those cards` and `the rest` of the following ordinary actions
/// read exactly those identities. A searched library is shuffled only by a
/// later written instruction (`then shuffle`, `Then that player
/// shuffles.`); a search without one is refused, never silently shuffled.
pub(in crate::oracle::canonical) fn observe_bound_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    collections: &mut CardCollectionMemory,
    search: &LibrarySearchEffectNode,
    later: &[LocalEffectNode],
) -> MachineResult<()> {
    if search.destination != SearchDestination::Bound {
        return Ok(());
    }
    cursor.enter("lower.library-search.bound-result.shuffle");
    cursor.require(
        !search.origin.zones.contains(&CardZoneName::Library)
            || later.iter().any(effect_shuffles_a_library),
        "a later instruction that shuffles the searched library (CR 701.23a)",
    )?;
    collections.observe_searched_cards(search_found_cards(search), search_found_maximum(search));
    Ok(())
}

/// Whether an action of the program shuffles a library: `shuffle`, a
/// player's own `shuffles`, a `shuffle ... into` move, or such an action
/// inside an optional block.
fn effect_shuffles_a_library(effect: &LocalEffectNode) -> bool {
    match effect {
        LocalEffectNode::CardAction(CardActionNode::Shuffle { .. }) => true,
        LocalEffectNode::CardAction(CardActionNode::Move { destination, .. }) => {
            destination.shuffle
        }
        LocalEffectNode::Performed(performed) => effect_shuffles_a_library(&performed.effect),
        LocalEffectNode::OptionalSequence(body) => {
            body.effects.iter().any(effect_shuffles_a_library)
        }
        _ => false,
    }
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

/// One choice per slot of `a Forest card and a Plains card` (CR 701.23a):
/// each slot finds at most one card of its own phrase among the searched
/// zones' cards that no earlier slot chose, so one card never fills two
/// slots. A hidden-zone search may find nothing for a slot.
fn search_slot_choices(
    search: &LibrarySearchEffectNode,
    slot_ids: &[String],
    player: &Value,
    holder: &Value,
    zones: &[CardZoneName],
) -> Vec<Value> {
    search
        .slots
        .iter()
        .zip(slot_ids)
        .enumerate()
        .map(|(index, (slot, id))| {
            let filter = lower_card_object_predicate(slot);
            let zone_query = |zone: &CardZoneName| {
                json!({
                    "kind": "cardsInZone",
                    "zone": {"kind": zone.ir_name(), "player": holder.clone()},
                    "where": filter.clone(),
                })
            };
            let mut candidates = match zones {
                [zone] => zone_query(zone),
                zones => json!({
                    "kind": "union",
                    "sets": zones.iter().map(zone_query).collect::<Vec<_>>(),
                }),
            };
            if index > 0 {
                candidates = json!({
                    "kind": "setDifference",
                    "left": candidates,
                    "right": {
                        "kind": "union",
                        "sets": slot_ids[..index]
                            .iter()
                            .map(|earlier| decision_result(earlier))
                            .collect::<Vec<_>>(),
                    },
                });
            }
            json!({
                "kind": "chooseCards",
                "id": id,
                "player": player.clone(),
                "minimum": 0,
                "maximum": 1,
                "from": candidates,
            })
        })
        .collect()
}

/// The move of the found cards to the search destination.
fn lower_search_move(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
    cards: Value,
    player: &Value,
    holder: &Value,
) -> MachineResult<Value> {
    Ok(json!({
        "kind": "moveCards",
        "cards": cards,
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
            SearchDestination::Bound => {
                return Err(
                    cursor.fail("a search destination; a bound result moves by later actions")
                );
            }
        },
    }))
}

#[cfg(test)]
#[path = "library_search_move_tests.rs"]
mod tests;
