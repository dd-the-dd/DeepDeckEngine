use super::super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

#[test]
fn this_creature_and_flying_selector_are_typed() {
    let tokens = recognize_entities("This creature can block only creatures with flying.");
    let mut cursor = OracleTokenCursor::new("block-restriction-test", &tokens);
    let restriction = recognize_block_only_restriction(&mut cursor, &[], None)
        .expect("block-only restriction parses");

    assert_eq!(restriction.source, LocalObjectReference::SourceSelf);
    assert_eq!(restriction.allowed_attackers.controller, None);
    assert_eq!(
        restriction.allowed_attackers.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::HasKeyword(GrantedKeyword::Flying),
        ])
    );
}

#[test]
fn direct_selector_recognizes_creatures_with_flying() {
    let tokens = recognize_entities("creatures with flying");
    let mut cursor = OracleTokenCursor::new("selector-test", &tokens);
    let query = recognize_object_query(&mut cursor).expect("selector parses");
    cursor.finish().expect("selector consumes its input");

    assert_eq!(
        query.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::HasKeyword(GrantedKeyword::Flying),
        ])
    );
}

#[test]
fn attack_near_miss_fails_in_the_action_state() {
    let tokens = recognize_entities("This creature can attack only creatures with flying.");
    let mut cursor = OracleTokenCursor::new("block-restriction-test", &tokens);
    let failure =
        recognize_block_only_restriction(&mut cursor, &[], None).expect_err("attack is not block");

    assert_eq!(failure.state, "block-restriction.action");
    assert_eq!(failure.expected, "the `block` action");
}

#[test]
fn unknown_keyword_fails_in_the_selector_keyword_state() {
    let tokens = recognize_entities("This creature can block only creatures with skybound.");
    let mut cursor = OracleTokenCursor::new("block-restriction-test", &tokens);
    let failure = recognize_block_only_restriction(&mut cursor, &[], None)
        .expect_err("unknown keyword is explicit");

    assert_eq!(failure.state, "selector.keyword-qualifier.keyword");
    assert_eq!(failure.actual.as_deref(), Some("skybound"));
}
