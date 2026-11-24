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
                additional_zones: Vec::new(),
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
                face_down: false,
                transformed: false,
                shuffle: false,
                attached_to: None,
                origin_library: false,
            },
        })
        .collect::<Vec<_>>();
    actions.push(CardActionNode::Shuffle {
        owner: LocalPlayerReference::Controller,
    });
    cursor.enter("effect.shuffle-into.complete");
    Ok(Some(actions))
}

/// `into its owner's library`, `into their owners' libraries`, `into your
/// library`, `into their library`: the library a shuffled object goes to.
/// Every card goes to its owner's library (CR 400.3) whatever possessive is
/// written, so the destination is always each moved card's owner's library,
/// placed in a random order and then shuffled (CR 701.24b).
pub(in crate::oracle::canonical) fn recognize_shuffle_library_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardMoveDestinationNode> {
    cursor.enter("effect.shuffle-into.destination");
    let destination = match try_recognize_card_move_destination(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("`into` a library after the shuffled object"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.enter("effect.shuffle-into.library");
    cursor.require(
        destination.zone == CardZoneName::Library
            && destination.position.is_none()
            && destination.order.is_none(),
        "an unpositioned library: a shuffle leaves no position or order",
    )?;
    Ok(CardMoveDestinationNode {
        zone: CardZoneName::Library,
        owner: CardMoveOwnerNode::EachCardOwner,
        position: Some(LibraryPosition::Bottom),
        order: Some(CollectionOrder::Random),
        tapped: false,
        entering_counters: None,
        face_down: false,
        transformed: false,
        shuffle: true,
        attached_to: None,
        origin_library: false,
    })
}

/// True when `shuffle` is followed by an object rather than ending its
/// instruction (`Then shuffle.`, `shuffle, then ...`, `shuffle your library`).
pub(in crate::oracle::canonical) fn shuffle_object_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        None | Some("." | "," | ";" | ")" | "and" | "then") => false,
        Some("your") => !cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("library")),
        Some(_) => true,
    }
}

/// `shuffle SOURCE into its owner's library`: the source object (a name
/// linked to the current card or face, or `this OBJECT`) moves to its
/// owner's library, which is then shuffled (CR 701.24b). `Ok(None)` when no
/// source reference follows the verb (the card-action reader owns `it`,
/// targets and collections); once the source is read, any other
/// destination fails here.
pub(in crate::oracle::canonical) fn try_recognize_shuffle_source_into_library(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<Option<MoveSourceEffectNode>> {
    if !cursor
        .peek()
        .is_some_and(|raw| is_verb_form(raw, VerbLemma::Shuffle, VerbForm::Base))
    {
        return Ok(None);
    }
    let mut trial = cursor.clone();
    trial.enter("effect.shuffle-source.verb");
    trial.consume(verb_entry(VerbLemma::Shuffle).surface(VerbForm::Base))?;
    let starts_source = trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        || name_links.iter().any(|link| {
            link.token_start == trial.position()
                && matches!(
                    link.kind,
                    crate::oracle::syntax::NameLinkKind::CurrentCard
                        | crate::oracle::syntax::NameLinkKind::CurrentFace
                )
        });
    if !starts_source {
        return Ok(None);
    }
    trial.enter("effect.shuffle-source.source");
    let source = recognize_source_reference(&mut trial, name_links, name_failure)?;
    recognize_shuffle_library_destination(&mut trial)?;
    trial.enter("effect.shuffle-source.complete");
    *cursor = trial;
    // The source moves from wherever it is (a permanent, a card in a
    // graveyard, or the resolving spell); no origin is written.
    Ok(Some(MoveSourceEffectNode {
        source,
        from: SourceZone::Battlefield,
        to: SourceZone::Library,
        face_change: SourceFaceChange::Preserve,
        controller: SourceMoveController::Owner,
        tapped: false,
        attached_to: None,
    }))
}

#[cfg(test)]
#[path = "shuffle_into_tests.rs"]
mod tests;
