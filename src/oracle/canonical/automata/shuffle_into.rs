//! `shuffle ZONES into LIBRARY`: the shuffle keyword action with an object.
//!
//! ```text
//! ShuffleInto := "shuffle" "your" ZoneList "into" "your" "library"
//! ZoneList    := the shared card zone list (`hand`, `graveyard`, coordinated)
//! ```
//!
//! "Shuffle X into a library" puts X into that library, then shuffles it
//! (CR 701.24a, 701.24b). Every card goes to its owner's library (CR 400.3),
//! so the action composes the ordinary card move of each zone's contents and
//! the ordinary shuffle of the acting player's library; no combined shuffle
//! operation exists. The acting player is the clause's: the imperative names
//! the controller, and a performer subject (`each player shuffles their hand
//! and graveyard into their library`) is read by the performer's person
//! shift, so the same reader serves every player subject.
//!
//! The reader is transactional: it claims only `shuffle your` followed by a
//! hand or graveyard. Shuffling a library alone (`shuffle your library`)
//! stays with the card-action reader. After the claim, every failure is this
//! frame's own explicit diagnostic.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// True when the cursor starts `shuffle your hand|graveyard ...`.
pub(in crate::oracle::canonical) fn shuffle_zones_into_library_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| is_verb_form(raw, VerbLemma::Shuffle, VerbForm::Base))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
        && cursor.peek_ahead(2).is_some_and(|raw| {
            ["hand", "graveyard"]
                .iter()
                .any(|zone| raw.eq_ignore_ascii_case(zone))
        })
}

/// `shuffle your ZONES into your library`: one card move per zone into its
/// owner's library, then the shuffle of the acting player's library, in
/// that order. `Ok(None)` when the frame does not start at the cursor.
pub(in crate::oracle::canonical) fn try_recognize_shuffle_zones_into_library(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<Vec<CardActionNode>>> {
    if !shuffle_zones_into_library_starts(cursor) {
        return Ok(None);
    }
    cursor.enter("effect.shuffle-into.verb");
    cursor.consume(verb_entry(VerbLemma::Shuffle).surface(VerbForm::Base))?;
    cursor.enter("effect.shuffle-into.zone-owner");
    cursor.consume("your")?;
    cursor.enter("effect.shuffle-into.zones");
    let zones = recognize_card_zone_list(cursor)?;
    cursor.enter("effect.shuffle-into.zone-kind");
    cursor.require(
        zones
            .iter()
            .all(|zone| matches!(zone, CardZoneName::Hand | CardZoneName::Graveyard)),
        "a hand or graveyard shuffled into a library",
    )?;
    cursor.enter("effect.shuffle-into.destination");
    cursor.consume_phrase(&["into", "your", "library"])?;
    let mut actions = zones
        .into_iter()
        .map(|zone| CardActionNode::Move {
            cards: CardOperandNode::ZoneCollection(CardZoneCollectionNode {
                zone,
                owner: CardZoneOwnerNode::Player(LocalPlayerReference::Controller),
                predicate: None,
                quantity: None,
            }),
            // The library is shuffled next: the placement order is not
            // observable, and each card goes to its owner's library.
            destination: CardMoveDestinationNode {
                zone: CardZoneName::Library,
                owner: CardMoveOwnerNode::EachCardOwner,
                position: Some(LibraryPosition::Bottom),
                order: Some(CollectionOrder::Random),
                tapped: false,
                entering_counters: None,
            },
        })
        .collect::<Vec<_>>();
    actions.push(CardActionNode::Shuffle {
        owner: LocalPlayerReference::Controller,
    });
    cursor.enter("effect.shuffle-into.complete");
    Ok(Some(actions))
}

#[cfg(test)]
#[path = "shuffle_into_tests.rs"]
mod tests;
