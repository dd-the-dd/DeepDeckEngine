//! The cast frame's transactional outcome and its collection origins: a
//! frame that read its origin owns its failure; a head without its cards is
//! left to other frames; a restated collection (`cards revealed this way`,
//! `those nonland cards`) is one origin of the same permitted query.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn frame() -> CardActionReadContext<'static> {
    CardActionReadContext {
        source_available: false,
        preposed_duration: None,
        name_links: &[],
        name_failure: None,
    }
}

fn outcome(text: &str) -> (CollectionOperandAttempt<CardActionNode>, usize) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("cast-frame-outcome-test", &tokens);
    let attempt = try_recognize_card_action_outcome(&mut cursor, false, frame());
    (attempt, cursor.position())
}

fn permitted_selection(text: &str) -> CardSelectionOperandNode {
    let (attempt, _) = outcome(text);
    let CollectionOperandAttempt::Recognized(fragment) = attempt else {
        panic!("{text}: the cast frame is recognized");
    };
    let CardActionNode::Permit(permit) = fragment.node else {
        panic!("{text}: a cast frame");
    };
    let CardOperandNode::Selection(selection) = permit.cards else {
        panic!("{text}: a selection from a bound collection");
    };
    selection
}

#[test]
fn a_cast_frame_that_read_its_origin_owns_its_failure() {
    for text in [
        // The permitted query read `from`; its cost tail is unsupported.
        "you may cast a spell from among them by paying nothing at all",
        // The cards were read and a base-cost tail only this frame reads
        // follows them.
        "you may cast them without paying its mana costs",
    ] {
        let (attempt, position) = outcome(text);
        assert!(
            matches!(attempt, CollectionOperandAttempt::FailedCommitted(_)),
            "{text}"
        );
        assert_eq!(position, 0, "{text}: a failed child never advances");
    }
}

#[test]
fn a_cast_head_without_its_cards_is_left_to_other_frames() {
    let (attempt, position) = outcome("you may cast spells this turn as though they had flash");
    assert!(matches!(
        attempt,
        CollectionOperandAttempt::RejectedCandidate(_)
    ));
    assert_eq!(position, 0);
    let (attempt, _) = outcome("you may draw a card");
    assert!(matches!(attempt, CollectionOperandAttempt::NotClaimed));
}

#[test]
fn a_complete_cast_frame_is_recognized_with_its_end() {
    let text = "you may cast them without paying their mana costs";
    let (attempt, position) = outcome(text);
    let CollectionOperandAttempt::Recognized(fragment) = attempt else {
        panic!("{text}");
    };
    assert_eq!(fragment.end_position, position);
    let CardActionNode::Permit(permit) = fragment.node else {
        panic!("a cast frame");
    };
    assert!(permit.without_paying_mana_cost());
}

#[test]
fn a_restated_collection_is_the_origin_of_the_permitted_query() {
    for (text, verb) in [
        (
            "you may cast a spell from among cards revealed this way",
            CardCollectionVerb::Revealed,
        ),
        (
            "you may cast any number of spells from among the cards exiled this way",
            CardCollectionVerb::Exiled,
        ),
    ] {
        let selection = permitted_selection(text);
        assert_eq!(
            selection.from,
            CardCollectionReferenceNode::Restated(verb),
            "{text}"
        );
    }
}

#[test]
fn a_restated_qualifier_narrows_the_permitted_query() {
    for text in [
        "you may cast any number of spells from among those nonland cards",
        "you may cast any number of spells from among the nonland cards exiled this way",
    ] {
        let selection = permitted_selection(text);
        let Some(ObjectPredicate::Intersection(operands)) = &selection.predicate else {
            panic!("{text}: the spell query intersected with the qualifier");
        };
        assert!(operands.len() >= 2, "{text}");
    }
    // The shared reference keeps its own reading: `those exiled cards` is
    // the program's exile, not a qualifier `exiled`.
    let selection = permitted_selection("you may cast a spell from among those exiled cards");
    assert_eq!(selection.from, CardCollectionReferenceNode::Exiled);
}

#[test]
fn an_unread_relation_after_the_origin_is_the_frames_failure() {
    // The origin was read (`from among the cards`); a relation the shared
    // reference does not read is the committed frame's failure, never a
    // silently dropped qualifier.
    let (attempt, position) =
        outcome("you may cast a spell from among the cards you own exiled this way");
    assert!(matches!(
        attempt,
        CollectionOperandAttempt::FailedCommitted(_)
    ));
    assert_eq!(position, 0);
}
