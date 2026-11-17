use crate::oracle::syntax::recognize_entities;

use super::super::{ObjectCardType, ObjectPredicate};
use super::{recognize_object_query, recognize_object_type_union_list, OracleTokenCursor};

fn recognize_complete_list(text: &str) -> super::super::super::MachineResult<ObjectPredicate> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-type-union-list-test", &tokens);
    let predicate = recognize_object_type_union_list(&mut cursor)?;
    cursor.finish()?;
    Ok(predicate)
}

#[test]
fn two_types_joined_by_and_are_a_collection_union() {
    assert_eq!(
        recognize_complete_list("artifact and enchantment").expect("two-type union parses"),
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
}

#[test]
fn oxford_list_of_plural_types_is_a_collection_union() {
    assert_eq!(
        recognize_complete_list("artifacts, creatures, and enchantments")
            .expect("Oxford type union parses"),
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
}

#[test]
fn list_stops_before_a_shared_postnominal_qualifier() {
    let tokens = recognize_entities(
        "artifact, creature, and enchantment with mana value less than or equal to X",
    );
    let mut cursor = OracleTokenCursor::new("selector-type-union-list-test", &tokens);

    let predicate = recognize_object_type_union_list(&mut cursor).expect("type union parses");

    assert_eq!(
        predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
    assert_eq!(cursor.peek(), Some("with"));
}

#[test]
fn adjacent_type_adjectives_remain_an_intersection_in_object_query() {
    let tokens = recognize_entities("artifact creature");
    let mut cursor = OracleTokenCursor::new("selector-adjective-intersection-test", &tokens);

    let query = recognize_object_query(&mut cursor).expect("adjective intersection parses");
    cursor
        .finish()
        .expect("object query consumes both adjectives");

    assert_eq!(
        query.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ])
    );
}

#[test]
fn dangling_oxford_connector_fails_at_the_missing_branch() {
    let tokens = recognize_entities("artifact, creature, and");
    let mut cursor = OracleTokenCursor::new("selector-type-union-list-test", &tokens);

    let failure = recognize_object_type_union_list(&mut cursor)
        .expect_err("dangling connector must not produce a partial union");

    assert_eq!(failure.state, "selector.type-union-list.branch");
    assert_eq!(
        failure.expected,
        "a registered singular or plural card type"
    );
    assert!(failure.actual.is_none());
}
