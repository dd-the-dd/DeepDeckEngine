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
