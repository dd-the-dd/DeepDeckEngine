use crate::oracle::syntax::recognize_entities;

use super::super::{CardSupertype, ObjectCardType, ObjectPredicate};
use super::{ChoiceDomainNode, ChooserNode, OracleTokenCursor, recognize_choose_instruction};

fn domain(text: &str) -> ChoiceDomainNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.choose-card-name", &tokens);
    let node = recognize_choose_instruction(&mut cursor).expect(text);
    assert_eq!(node.chooser, ChooserNode::Controller);
    assert_eq!(cursor.peek(), Some("."), "{text}");
    node.domain
}

fn not(card_type: ObjectCardType) -> ObjectPredicate {
    ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(card_type)))
}

/// Positive card shape (Cabal Therapy): one characteristic of the named card.
#[test]
fn a_qualified_card_name_choice_reads_the_shared_characteristic() {
    let ChoiceDomainNode::CardName(node) = domain("Choose a nonland card name.") else {
        panic!("a card-name domain");
    };
    assert_eq!(node.predicate, Some(not(ObjectCardType::Land)));
    let ChoiceDomainNode::CardName(node) = domain("Choose a card name.") else {
        panic!("an unrestricted card-name domain");
    };
    assert_eq!(node.predicate, None);
}

/// Siblings: a comma list of characteristics is their intersection (Lost
/// Legacy), and the basic-land exclusion composes with them.
#[test]
fn characteristic_lists_and_the_basic_land_exclusion_compose() {
    let ChoiceDomainNode::CardName(node) = domain("Choose a nonartifact, nonland card name.")
    else {
        panic!("a card-name domain");
    };
    assert_eq!(
        node.predicate,
        Some(ObjectPredicate::Intersection(vec![
            not(ObjectCardType::Artifact),
            not(ObjectCardType::Land),
        ]))
    );
    let ChoiceDomainNode::CardName(node) =
        domain("Choose a creature card name other than a basic land card name.")
    else {
        panic!("a card-name domain");
    };
    assert_eq!(
        node.predicate,
        Some(ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::Not(Box::new(ObjectPredicate::Intersection(vec![
                ObjectPredicate::Supertype(CardSupertype::Basic),
                ObjectPredicate::CardType(ObjectCardType::Land),
            ]))),
        ]))
    );
}

/// Near-misses: a history-qualified name choice and an unknown
/// characteristic fail in the card-name domain's own states.
#[test]
fn qualified_or_unknown_name_domains_fail_in_their_own_states() {
    for (text, state) in [
        (
            "Choose a card name that hasn't been chosen.",
            "choose.domain.card-name.boundary",
        ),
        ("Choose a gizmo card name.", "selector.predicate-atom.typed"),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.choose-card-name", &tokens);
        let failure = recognize_choose_instruction(&mut cursor).expect_err(text);
        assert!(
            failure.state.starts_with(state),
            "{text}: {}",
            failure.state
        );
    }
}
