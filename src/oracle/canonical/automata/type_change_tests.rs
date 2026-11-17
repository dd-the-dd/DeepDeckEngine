use crate::oracle::characteristic_vocabulary::BasicLandSubtype;
use crate::oracle::syntax::recognize_entities;

use super::{
    CardSupertype, ObjectCardType, OracleTokenCursor, TypeChangeCharacteristicsNode,
    TypeChangeNumber, TypeChangeRetention, TypeChangeSubtype,
    recognize_type_change_characteristics,
};

fn parse(text: &str, number: TypeChangeNumber) -> Result<TypeChangeCharacteristicsNode, String> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.type-change", &tokens);
    let node = recognize_type_change_characteristics(&mut cursor, number)
        .map_err(|failure| format!("{}: {}", failure.state, failure.expected))?;
    cursor.consume(".").map_err(|failure| failure.expected)?;
    Ok(node)
}

#[test]
fn plural_bare_basic_land_subtype_replaces_its_class() {
    let node = parse("Mountains.", TypeChangeNumber::Plural).expect("plural land subtype");
    assert_eq!(
        node.subtypes,
        [TypeChangeSubtype::BasicLand(BasicLandSubtype::Mountain)]
    );
    assert_eq!(node.retention, TypeChangeRetention::ReplacesSameClass);
}

#[test]
fn singular_article_subtype_with_land_type_retention() {
    let node = parse(
        "a Forest in addition to its other land types.",
        TypeChangeNumber::Singular,
    )
    .expect("additive land subtype");
    assert_eq!(
        node.subtypes,
        [TypeChangeSubtype::BasicLand(BasicLandSubtype::Forest)]
    );
    assert_eq!(node.retention, TypeChangeRetention::InAdditionToOtherTypes);
}

#[test]
fn every_basic_land_type_and_prenominal_subtype_before_plural_card_type() {
    let node = parse(
        "every basic land type in addition to their other types.",
        TypeChangeNumber::Plural,
    )
    .expect("all basic land types");
    assert!(node.all_basic_land_types);
    assert_eq!(node.retention, TypeChangeRetention::InAdditionToOtherTypes);

    let node = parse(
        "Forest lands in addition to their other types.",
        TypeChangeNumber::Plural,
    )
    .expect("subtype modifier then plural card type");
    assert_eq!(
        node.subtypes,
        [TypeChangeSubtype::BasicLand(BasicLandSubtype::Forest)]
    );
    assert_eq!(node.card_types, [ObjectCardType::Land]);
}

#[test]
fn supertype_adjective_and_creature_subtype() {
    let node = parse("legendary.", TypeChangeNumber::Singular).expect("supertype adjective");
    assert_eq!(node.supertypes, [CardSupertype::Legendary]);
    assert!(!node.is_nominal());

    let node = parse(
        "an Angel in addition to its other types.",
        TypeChangeNumber::Singular,
    )
    .expect("creature subtype");
    assert_eq!(node.subtypes.len(), 1);
    assert_eq!(node.subtypes[0].ir_name(), "Angel");
}

#[test]
fn near_misses_keep_number_agreement_and_retained_class() {
    // A singular subject needs its article; a plural subject takes no article.
    assert!(parse("Mountain.", TypeChangeNumber::Singular).is_err());
    assert!(parse("a Mountain.", TypeChangeNumber::Plural).is_err());
    // The possessive agrees with the subject number.
    assert!(
        parse(
            "Islands in addition to its other types.",
            TypeChangeNumber::Plural
        )
        .is_err()
    );
    // `its other land types` cannot retain a card-type addition.
    assert!(
        parse(
            "an artifact in addition to its other land types.",
            TypeChangeNumber::Singular
        )
        .is_err()
    );
    // A nonpermanent card type cannot become a permanent's type.
    assert!(parse("instants.", TypeChangeNumber::Plural).is_err());
}
