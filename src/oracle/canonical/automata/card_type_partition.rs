use crate::oracle::{
    characteristic_vocabulary::{AcceptedForms, ORACLE_CARD_TYPES, lookup_card_type},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, NumericAtom, ObjectCardType, recognize_numeric_atom};
use super::{LocalTriggerEventNode, recognize_source_entered_event};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardTypePartitionSequenceNode {
    pub source: LocalObjectReference,
    pub count: i64,
    pub card_types: Vec<ObjectCardType>,
    pub reminder_card_types: Option<Vec<ObjectCardType>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardTypeReminderState {
    Open,
    Item,
    SeparatorOrPredicate,
    Predicate,
    Terminator,
    Close,
    Complete,
}

fn recognize_card_type_atom(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectCardType> {
    cursor.enter("card-type.atom");
    let token = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a known Oracle card type"))?;
    let card_type = lookup_card_type(token, AcceptedForms::SingularOnly)
        .ok_or_else(|| cursor.fail("a known Oracle card type"))?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(card_type)
}

fn recognize_card_type_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<ObjectCardType>> {
    let mut state = CardTypeReminderState::Open;
    let mut card_types = Vec::new();
    let mut final_item_follows = false;

    loop {
        state = match state {
            CardTypeReminderState::Open => {
                cursor.enter("reminder.card-types.open");
                cursor.consume("(")?;
                CardTypeReminderState::Item
            }
            CardTypeReminderState::Item => {
                cursor.enter("reminder.card-types.item");
                let card_type = recognize_card_type_atom(cursor)?;
                cursor.enter("reminder.card-types.unique");
                cursor.require(
                    !card_types.contains(&card_type),
                    "each reminder card type exactly once",
                )?;
                card_types.push(card_type);
                if final_item_follows {
                    CardTypeReminderState::Predicate
                } else {
                    CardTypeReminderState::SeparatorOrPredicate
                }
            }
            CardTypeReminderState::SeparatorOrPredicate => {
                cursor.enter("reminder.card-types.separator");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    {
                        cursor.consume("and")?;
                        final_item_follows = true;
                    }
                    CardTypeReminderState::Item
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    final_item_follows = true;
                    CardTypeReminderState::Item
                } else {
                    CardTypeReminderState::Predicate
                }
            }
            CardTypeReminderState::Predicate => {
                cursor.enter("reminder.card-types.predicate");
                cursor.consume_phrase(&["are", "card", "types"])?;
                CardTypeReminderState::Terminator
            }
            CardTypeReminderState::Terminator => {
                cursor.enter("reminder.card-types.terminator");
                cursor.consume(".")?;
                CardTypeReminderState::Close
            }
            CardTypeReminderState::Close => {
                cursor.enter("reminder.card-types.close");
                cursor.consume(")")?;
                CardTypeReminderState::Complete
            }
            CardTypeReminderState::Complete => {
                cursor.enter("reminder.card-types.complete");
                return Ok(card_types);
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardTypePartitionState {
    Trigger,
    Event,
    TriggerSeparator,
    RevealAction,
    TopCollection,
    Count,
    CardNoun,
    LibrarySource,
    RevealTerminator,
    SelectionQuantifier,
    SelectionCategory,
    SelectionSeparator,
    Player,
    Modality,
    PutAction,
    CardArticle,
    CardReference,
    RevealedRelation,
    RevealedCollection,
    HandDestination,
    SelectionTerminator,
    RemainderAction,
    RemainderReference,
    LibraryBottom,
    RandomOrder,
    RemainderTerminator,
    ReminderOrComplete,
    Complete,
}

/// Recognizes a source-entry sequence that reveals a bounded top-card
/// collection, makes one optional selection for each shared Oracle card type,
/// and bottoms the exact remainder in random order.
pub(in crate::oracle::canonical) fn recognize_card_type_partition_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<CardTypePartitionSequenceNode> {
    let mut state = CardTypePartitionState::Trigger;
    let mut source = None;
    let mut count = None;
    let mut reminder_card_types = None;

    loop {
        state = match state {
            CardTypePartitionState::Trigger => {
                cursor.enter("trigger.card-type-partition.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the `when` trigger",
                )?;
                CardTypePartitionState::Event
            }
            CardTypePartitionState::Event => {
                cursor.enter("trigger.card-type-partition.event");
                let LocalTriggerEventNode::SourceEntered {
                    source: entered_source,
                } = recognize_source_entered_event(cursor, name_links, name_failure)?
                else {
                    unreachable!("source-entry child returns its typed event")
                };
                source = Some(entered_source);
                CardTypePartitionState::TriggerSeparator
            }
            CardTypePartitionState::TriggerSeparator => {
                cursor.enter("trigger.card-type-partition.separator");
                cursor.consume(",")?;
                CardTypePartitionState::RevealAction
            }
            CardTypePartitionState::RevealAction => {
                cursor.enter("effect.reveal-top.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Reveal,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                CardTypePartitionState::TopCollection
            }
            CardTypePartitionState::TopCollection => {
                cursor.enter("effect.reveal-top.collection");
                cursor.consume_phrase(&["the", "top"])?;
                CardTypePartitionState::Count
            }
            CardTypePartitionState::Count => {
                cursor.enter("effect.reveal-top.count");
                let NumericAtom::Integer(value) = recognize_numeric_atom(cursor)? else {
                    return Err(cursor.fail("a fixed positive top-card count"));
                };
                cursor.require(value > 0, "a fixed positive top-card count")?;
                count = Some(value);
                CardTypePartitionState::CardNoun
            }
            CardTypePartitionState::CardNoun => {
                cursor.enter("effect.reveal-top.card-noun");
                if count == Some(1) {
                    cursor.consume("card")?;
                } else {
                    cursor.consume("cards")?;
                }
                CardTypePartitionState::LibrarySource
            }
            CardTypePartitionState::LibrarySource => {
                cursor.enter("effect.reveal-top.source");
                cursor.consume_phrase(&["of", "your", "library"])?;
                CardTypePartitionState::RevealTerminator
            }
            CardTypePartitionState::RevealTerminator => {
                cursor.enter("effect.reveal-top.terminator");
                cursor.consume(".")?;
                CardTypePartitionState::SelectionQuantifier
            }
            CardTypePartitionState::SelectionQuantifier => {
                cursor.enter("selection.per-card-type.quantifier");
                cursor.consume_phrase(&["for", "each"])?;
                CardTypePartitionState::SelectionCategory
            }
            CardTypePartitionState::SelectionCategory => {
                cursor.enter("selection.per-card-type.category");
                cursor.consume_phrase(&["card", "type"])?;
                CardTypePartitionState::SelectionSeparator
            }
            CardTypePartitionState::SelectionSeparator => {
                cursor.enter("selection.per-card-type.separator");
                cursor.consume(",")?;
                CardTypePartitionState::Player
            }
            CardTypePartitionState::Player => {
                cursor.enter("selection.per-card-type.player");
                cursor.consume("you")?;
                CardTypePartitionState::Modality
            }
            CardTypePartitionState::Modality => {
                cursor.enter("selection.per-card-type.modality");
                cursor.consume("may")?;
                CardTypePartitionState::PutAction
            }
            CardTypePartitionState::PutAction => {
                cursor.enter("selection.per-card-type.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                CardTypePartitionState::CardArticle
            }
            CardTypePartitionState::CardArticle => {
                cursor.enter("selection.per-card-type.article");
                cursor.consume("a")?;
                CardTypePartitionState::CardReference
            }
            CardTypePartitionState::CardReference => {
                cursor.enter("selection.per-card-type.card-reference");
                cursor.consume_phrase(&["card", "of", "that", "type"])?;
                CardTypePartitionState::RevealedRelation
            }
            CardTypePartitionState::RevealedRelation => {
                cursor.enter("selection.per-card-type.revealed-relation");
                cursor.consume_phrase(&["from", "among"])?;
                CardTypePartitionState::RevealedCollection
            }
            CardTypePartitionState::RevealedCollection => {
                cursor.enter("selection.per-card-type.revealed-collection");
                cursor.consume_phrase(&["the", "revealed", "cards"])?;
                CardTypePartitionState::HandDestination
            }
            CardTypePartitionState::HandDestination => {
                cursor.enter("selection.per-card-type.destination");
                cursor.consume_phrase(&["into", "your", "hand"])?;
                CardTypePartitionState::SelectionTerminator
            }
            CardTypePartitionState::SelectionTerminator => {
                cursor.enter("selection.per-card-type.terminator");
                cursor.consume(".")?;
                CardTypePartitionState::RemainderAction
            }
            CardTypePartitionState::RemainderAction => {
                cursor.enter("partition.remainder.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                CardTypePartitionState::RemainderReference
            }
            CardTypePartitionState::RemainderReference => {
                cursor.enter("partition.remainder.reference");
                cursor.consume_phrase(&["the", "rest"])?;
                CardTypePartitionState::LibraryBottom
            }
            CardTypePartitionState::LibraryBottom => {
                cursor.enter("partition.remainder.destination");
                cursor.consume_phrase(&["on", "the", "bottom", "of", "your", "library"])?;
                CardTypePartitionState::RandomOrder
            }
            CardTypePartitionState::RandomOrder => {
                cursor.enter("partition.remainder.order");
                cursor.consume_phrase(&["in", "a", "random", "order"])?;
                CardTypePartitionState::RemainderTerminator
            }
            CardTypePartitionState::RemainderTerminator => {
                cursor.enter("partition.remainder.terminator");
                cursor.consume(".")?;
                CardTypePartitionState::ReminderOrComplete
            }
            CardTypePartitionState::ReminderOrComplete => {
                cursor.enter("sequence.card-type-partition.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    reminder_card_types = Some(recognize_card_type_reminder(cursor)?);
                }
                CardTypePartitionState::Complete
            }
            CardTypePartitionState::Complete => {
                cursor.enter("sequence.card-type-partition.complete");
                cursor.finish()?;
                return Ok(CardTypePartitionSequenceNode {
                    source: cursor.require_value(source, "the entering source")?,
                    count: cursor.require_value(count, "a positive top-card count")?,
                    card_types: ORACLE_CARD_TYPES.to_vec(),
                    reminder_card_types,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "card_type_partition_tests.rs"]
mod tests;
