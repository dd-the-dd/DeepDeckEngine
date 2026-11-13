use crate::oracle::syntax::recognize_entities;

use super::super::{
    CardEntryDestinationNode, GraveyardCardSourceNode, LocalPlayerReference, NumericExpression,
    ObjectCardType, ObjectKind, ObjectPredicate,
};
use super::{OracleTokenCursor, recognize_graveyard_card_entry_life};

#[test]
fn creature_from_a_graveyard_links_mana_value_to_the_target() {
    let tokens = recognize_entities(
        "Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value.",
    );
    let mut cursor = OracleTokenCursor::new("graveyard-card-entry-life-test", &tokens);

    let sequence = recognize_graveyard_card_entry_life(&mut cursor)
        .expect("targeted graveyard entry and linked life loss parse");

    assert_eq!(sequence.entry.source, GraveyardCardSourceNode::AnyGraveyard);
    assert_eq!(
        sequence.entry.destination,
        CardEntryDestinationNode::Battlefield
    );
    assert_eq!(sequence.entry.controller, LocalPlayerReference::Controller);
    assert_eq!(
        sequence.entry.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ])
    );
    assert_eq!(
        sequence.life.amount,
        NumericExpression::ManaValueOf(sequence.target_binding)
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn card_type_and_graveyard_quantifier_are_reusable_sibling_slots() {
    let tokens = recognize_entities(
        "Put target artifact card from any graveyard to the battlefield under your control. You lose life equal to that card's mana value.",
    );
    let mut cursor = OracleTokenCursor::new("graveyard-card-entry-life-test", &tokens);

    let sequence = recognize_graveyard_card_entry_life(&mut cursor)
        .expect("a sibling card type and source quantifier parse");

    assert_eq!(
        sequence.entry.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::CardType(ObjectCardType::Artifact),
        ])
    );
}

#[test]
fn unsupported_destination_fails_at_the_local_destination_state() {
    let tokens = recognize_entities(
        "Put target creature card from a graveyard into your hand. You lose life equal to that card's mana value.",
    );
    let mut cursor = OracleTokenCursor::new("graveyard-card-entry-life-test", &tokens);

    let failure = recognize_graveyard_card_entry_life(&mut cursor)
        .expect_err("a different destination must not be approximated");

    assert_eq!(
        failure.state,
        "effect.graveyard-card-entry.destination-relation"
    );
    assert_eq!(failure.actual.as_deref(), Some("into"));
}
