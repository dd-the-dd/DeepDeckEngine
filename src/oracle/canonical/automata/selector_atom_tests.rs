use crate::oracle::characteristic_vocabulary::AcceptedForms;
use crate::oracle::syntax::recognize_entities;

use super::super::{CardColor, ObjectCardType, ObjectPredicate};
use super::{OracleTokenCursor, positive_object_selector_atom, recognize_object_predicate_atom};
use crate::oracle::canonical::recognize_alternative_return_cost_envelope;

fn parse(text: &str, forms: AcceptedForms) -> super::MachineResult<ObjectPredicate> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-predicate-atom-test", &tokens);
    let predicate = recognize_object_predicate_atom(&mut cursor, forms)?;
    cursor.finish()?;
    Ok(predicate)
}

#[test]
fn compact_type_and_color_negation_share_one_typed_atom_machine() {
    for (text, expected) in [
        (
            "noncreature",
            ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(
                ObjectCardType::Creature,
            ))),
        ),
        (
            "non-blue",
            ObjectPredicate::Not(Box::new(ObjectPredicate::Color(CardColor::Blue))),
        ),
        (
            "non-Wizard",
            ObjectPredicate::Not(Box::new(ObjectPredicate::Subtype("Wizard".to_string()))),
        ),
    ] {
        assert_eq!(
            parse(text, AcceptedForms::SingularOnly).expect("negated atom parses"),
            expected,
        );
    }
}

#[test]
fn card_types_colors_and_registered_subtypes_are_typed_atoms() {
    for (text, expected) in [
        (
            "artifact",
            ObjectPredicate::CardType(ObjectCardType::Artifact),
        ),
        ("green", ObjectPredicate::Color(CardColor::Green)),
        ("Wizard", ObjectPredicate::Subtype("Wizard".to_string())),
    ] {
        assert_eq!(
            parse(text, AcceptedForms::SingularOnly).expect("typed atom parses"),
            expected,
        );
    }
}

#[test]
fn plural_and_multiword_subtypes_use_the_versioned_vocabulary() {
    assert_eq!(
        parse("Sharks", AcceptedForms::SingularOrPlural).expect("registered plural subtype parses"),
        ObjectPredicate::Subtype("Shark".to_string()),
    );

    let tokens = recognize_entities("Time Lord spell");
    let mut cursor = OracleTokenCursor::new("selector-predicate-atom-test", &tokens);
    assert_eq!(
        recognize_object_predicate_atom(&mut cursor, AcceptedForms::SingularOnly)
            .expect("multiword subtype parses"),
        ObjectPredicate::Subtype("Time Lord".to_string()),
    );
    assert_eq!(cursor.peek(), Some("spell"));
}

#[test]
fn all_basic_land_subtypes_respect_the_parent_accepted_forms() {
    for (singular, plural, canonical) in [
        ("Plains", "Plains", "Plains"),
        ("Island", "Islands", "Island"),
        ("Swamp", "Swamps", "Swamp"),
        ("Mountain", "Mountains", "Mountain"),
        ("Forest", "Forests", "Forest"),
    ] {
        let expected = ObjectPredicate::Subtype(canonical.to_string());
        assert_eq!(
            parse(singular, AcceptedForms::SingularOnly)
                .expect("registered singular basic land subtype parses"),
            expected,
        );
        assert_eq!(
            parse(plural, AcceptedForms::SingularOrPlural)
                .expect("registered plural basic land subtype parses"),
            expected,
        );
    }

    for plural in ["Islands", "Swamps", "Mountains", "Forests"] {
        let failure = parse(plural, AcceptedForms::SingularOnly)
            .expect_err("plural basic land subtype requires a plural-capable parent");
        assert_eq!(failure.actual.as_deref(), Some(plural));
    }
}

#[test]
fn compatibility_selector_entry_maps_basic_land_subtypes_to_the_same_ir() {
    for (surface, canonical) in [
        ("Plains", "Plains"),
        ("Island", "Island"),
        ("Islands", "Island"),
        ("Swamp", "Swamp"),
        ("Swamps", "Swamp"),
        ("Mountain", "Mountain"),
        ("Mountains", "Mountain"),
        ("Forest", "Forest"),
        ("Forests", "Forest"),
    ] {
        assert_eq!(
            positive_object_selector_atom(surface),
            Some(ObjectPredicate::Subtype(canonical.to_string())),
        );
    }
    assert_eq!(positive_object_selector_atom("Islander"), None);
}

#[test]
fn alternative_return_cost_reuses_the_basic_land_subtype_atom() {
    let tokens = recognize_entities(
        "You may return an Island you control to its owner's hand rather than pay this spell's mana cost.",
    );
    let mut cursor = OracleTokenCursor::new("alternative-return-basic-land-test", &tokens);
    let envelope = recognize_alternative_return_cost_envelope(&mut cursor)
        .expect("the alternative-return parent delegates its selector atom");

    assert_eq!(
        envelope.cost.predicate,
        ObjectPredicate::Subtype("Island".to_string()),
    );
}

#[test]
fn unknown_words_are_near_misses() {
    for text in ["nonlesson", "target", "Islander"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selector-predicate-atom-test", &tokens);
        let failure = recognize_object_predicate_atom(&mut cursor, AcceptedForms::SingularOnly)
            .expect_err("unsupported atom must remain visible");

        assert_eq!(failure.state, "selector.predicate-atom.typed");
        assert_eq!(cursor.position(), 0);
    }
}

#[test]
fn singular_mode_does_not_silently_accept_plural_subtypes() {
    let failure = parse("Sharks", AcceptedForms::SingularOnly)
        .expect_err("plural surface requires an explicit plural-capable parent");

    assert_eq!(failure.actual.as_deref(), Some("Sharks"));
}
