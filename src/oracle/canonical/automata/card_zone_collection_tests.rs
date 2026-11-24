use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn collection_owns_zone_owner_and_predicate_but_not_action_connectors() {
    for (text, targeted, filtered, zone) in [
        (
            "target player's graveyard then",
            true,
            false,
            CardZoneName::Graveyard,
        ),
        (
            "target opponent's hand and",
            true,
            false,
            CardZoneName::Hand,
        ),
        ("your exile then", false, false, CardZoneName::Exile),
        (
            "all instant or sorcery cards in your graveyard and",
            false,
            true,
            CardZoneName::Graveyard,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-query", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_zone_collection(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(fragment.node.zone, zone);
        assert_eq!(fragment.node.predicate.is_some(), filtered);
        assert_eq!(
            matches!(fragment.node.owner, CardZoneOwnerNode::TargetPlayer(_)),
            targeted
        );
        assert!(matches!(cursor.peek(), Some("then" | "and")));
        assert_eq!(fragment.end_position, cursor.position());
    }
}

#[test]
fn unsupported_or_incomplete_zone_heads_are_transactional() {
    for text in [
        "target player's unknown",
        "all creatures in your graveyard",
        "target player graveyard",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-query-negative", &tokens);
        assert!(
            !matches!(
                try_recognize_card_zone_collection(&mut cursor),
                CollectionOperandAttempt::Recognized(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0);
    }
}

/// A population of owners reads the same public zone of every player it
/// names; a hidden zone of other players is not an open collection.
#[test]
fn a_population_of_graveyard_owners_is_one_zone_query() {
    for (text, population) in [
        (
            "all cards from all graveyards then",
            PlayerSetReferenceNode::EachPlayer,
        ),
        (
            "a creature card from a graveyard then",
            PlayerSetReferenceNode::EachPlayer,
        ),
        (
            "an artifact card from an opponent's graveyard then",
            PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
        (
            "all creature cards from all opponents' graveyards then",
            PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-population", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_zone_collection(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(fragment.node.zone, CardZoneName::Graveyard, "{text}");
        assert_eq!(
            fragment.node.owner,
            CardZoneOwnerNode::Players(population),
            "{text}"
        );
        assert_eq!(cursor.peek(), Some("then"), "{text}");
    }
    let tokens = recognize_entities("all cards from all hands");
    let mut cursor = OracleTokenCursor::new("zone-population-negative", &tokens);
    assert!(matches!(
        try_recognize_card_zone_collection(&mut cursor),
        CollectionOperandAttempt::FailedCommitted(_)
    ));
    assert_eq!(cursor.position(), 0);
}

/// `from your hand or graveyard`: one owner, a union of zones. A connector
/// before anything other than a zone stays with the parent.
#[test]
fn coordinated_owned_zones_extend_one_collection_without_taking_the_connector() {
    let tokens = recognize_entities("a land card from your hand or graveyard onto");
    let mut cursor = OracleTokenCursor::new("zone-union", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_zone_collection(&mut cursor)
    else {
        panic!("a coordinated zone collection")
    };
    assert_eq!(fragment.node.zone, CardZoneName::Hand);
    assert_eq!(
        fragment.node.additional_zones,
        vec![CardZoneName::Graveyard]
    );
    assert_eq!(cursor.peek(), Some("onto"));

    let tokens = recognize_entities("all cards from your graveyard and draw");
    let mut cursor = OracleTokenCursor::new("zone-union-boundary", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_zone_collection(&mut cursor)
    else {
        panic!("a single zone collection")
    };
    assert!(fragment.node.additional_zones.is_empty());
    assert_eq!(cursor.peek(), Some("and"));
}

/// The card noun may be separated from its owned zone by a coordinated type
/// list or a postposed qualifier; the claim still names the zone collection.
#[test]
fn a_comma_type_list_or_qualifier_does_not_hide_the_owned_zone() {
    for text in [
        "an artifact, creature, enchantment, or land card from your hand onto",
        "a creature card with mana value 3 or less from your graveyard onto",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("zone-start", &tokens);
        assert!(chosen_zone_cards_start(&cursor), "{text}");
    }
}
