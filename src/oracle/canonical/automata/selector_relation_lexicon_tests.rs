//! One relation lexicon in the object noun phrase: the verb names the
//! relation, the parent's projection only type-checks it.
use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn noun_query(text: &str) -> (RecognizedObjectQuery, Vec<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-relation-lexicon", &tokens);
    let query = recognize_object_noun_query(&mut cursor, ObjectQueryContext::ORDINARY)
        .unwrap_or_else(|failure| panic!("{text}: {failure}"));
    let rest = tokens[cursor.position()..]
        .iter()
        .map(|token| token.raw.clone())
        .collect();
    (query, rest)
}

#[test]
fn every_relation_verb_is_read_by_the_noun_phrase_itself() {
    for (text, controller, caster, owner) in [
        ("creatures you control", true, false, false),
        ("spells an opponent casts", false, true, false),
        ("permanents your opponents own", false, false, true),
        ("cards you don't own", false, false, true),
    ] {
        let (query, rest) = noun_query(text);
        assert!(rest.is_empty(), "{text}: {rest:?}");
        let relations = query.relations();
        assert_eq!(relations.has_controller(), controller, "{text}");
        assert_eq!(relations.caster.is_some(), caster, "{text}");
        assert_eq!(relations.owner.is_some(), owner, "{text}");
    }
}

#[test]
fn ownership_by_you_stays_the_owned_by_characteristic_for_control_consumers() {
    let tokens = recognize_entities("creatures you own");
    let mut cursor = OracleTokenCursor::new("selector-relation-owned", &tokens);
    let query = recognize_object_query(&mut cursor).expect("an owned collection");
    assert_eq!(query.controller, None);
    assert_eq!(
        query.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::OwnedByAbilityController { negated: false },
        ])
    );
}

#[test]
fn control_and_ownership_compose_in_a_control_consumer() {
    // Unused combination: an inherited `but don't own` relation read by the
    // ordinary control consumer, with no production code of its own.
    let tokens = recognize_entities("creatures you control but don't own");
    let mut cursor = OracleTokenCursor::new("selector-relation-but", &tokens);
    let query = recognize_object_query(&mut cursor).expect("control but not ownership");
    assert_eq!(cursor.position(), tokens.len());
    assert_eq!(
        query.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(
        query.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::OwnedByAbilityController { negated: true },
        ])
    );
    // The related consumer keeps the same ownership as a relation.
    let (related, _) = noun_query("creatures you control but don't own");
    let tokens = recognize_entities("x");
    let mut cursor = OracleTokenCursor::new("selector-relation-related", &tokens);
    let related = related.into_related_query(&mut cursor).expect("relations");
    assert_eq!(
        related.ownership,
        Some(ObjectOwnershipScope::NotAbilityController)
    );
}

#[test]
fn a_but_without_a_relation_belongs_to_the_parent() {
    let (query, rest) = noun_query("creatures you control but can't block");
    assert!(query.relations().has_controller());
    assert_eq!(rest.first().map(String::as_str), Some("but"));
}

#[test]
fn a_relation_the_consumer_cannot_represent_fails_at_its_verb() {
    let tokens = recognize_entities("permanent an opponent owns");
    let mut cursor = OracleTokenCursor::new("selector-relation-refused", &tokens);
    let failure = recognize_object_query(&mut cursor).expect_err("owner is not a controller");
    assert_eq!(
        failure.state,
        "selector.controller-qualifier.relation-or-negation"
    );
    assert_eq!(failure.actual.as_deref(), Some("owns"));
    assert_eq!(failure.expected, "controls");

    let tokens = recognize_entities("spells you control");
    let mut cursor = OracleTokenCursor::new("selector-relation-cast-refused", &tokens);
    let failure = recognize_object_noun_query(&mut cursor, ObjectQueryContext::ORDINARY)
        .and_then(|query| query.into_spell_cast_query(&mut cursor))
        .expect_err("control is not the caster relation");
    assert_eq!(failure.actual.as_deref(), Some("control"));
    assert_eq!(failure.expected, "cast");
}
