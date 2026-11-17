use crate::oracle::syntax::recognize_entities;

use super::super::{ObjectCardType, ObjectPredicate};
use super::{OracleTokenCursor, recognize_object_query};

fn query(text: &str) -> super::super::super::MachineResult<super::super::ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-noncreature-subtype-test", &tokens);
    let node = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(node)
}

fn subtype(name: &str) -> ObjectPredicate {
    ObjectPredicate::Subtype(name.to_string())
}

#[test]
fn artifact_subtype_is_an_object_head_in_singular_and_plural() {
    for (text, name) in [
        ("Vehicle you control", "Vehicle"),
        ("Vehicles you control", "Vehicle"),
        ("Treasure you control", "Treasure"),
        ("Equipment you control", "Equipment"),
        ("Clues you control", "Clue"),
        ("Spacecraft you control", "Spacecraft"),
    ] {
        assert_eq!(query(text).expect(text).predicate, subtype(name), "{text}");
    }
}

#[test]
fn subtype_families_other_than_artifact_share_the_head() {
    for (text, name) in [
        ("Auras you control", "Aura"),
        ("Saga you control", "Saga"),
        ("Gates you control", "Gate"),
        ("Jace you control", "Jace"),
    ] {
        assert_eq!(query(text).expect(text).predicate, subtype(name), "{text}");
    }
}

#[test]
fn subtype_composes_with_type_and_status_atoms_without_new_grammar() {
    assert_eq!(
        query("tapped Treasures you control")
            .expect("status plus subtype")
            .predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Status(super::super::ObjectStatus::Tapped),
            subtype("Treasure"),
        ])
    );
    assert_eq!(
        query("artifact Vehicle you control")
            .expect("type plus subtype")
            .predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            subtype("Vehicle"),
        ])
    );
}

#[test]
fn creatures_and_or_subtype_is_a_union() {
    assert_eq!(
        query("creatures and/or Treasures you control")
            .expect("type and subtype union")
            .predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            subtype("Treasure"),
        ])
    );
}

#[test]
fn lowercase_words_and_non_subtypes_are_not_subtype_nouns() {
    for text in [
        "case you control",
        "map you control",
        "Treasurex you control",
    ] {
        assert!(query(text).is_err(), "{text} must fail explicitly");
    }
}
