use super::*;
use crate::oracle::syntax::recognize_entities;

fn zone_moves(actions: &[CardActionNode]) -> Vec<CardZoneName> {
    actions
        .iter()
        .filter_map(|action| match action {
            CardActionNode::Move {
                cards: CardOperandNode::ZoneCollection(collection),
                destination,
            } => {
                assert_eq!(destination.zone, CardZoneName::Library);
                assert_eq!(destination.owner, CardMoveOwnerNode::EachCardOwner);
                assert_eq!(destination.order, Some(CollectionOrder::Random));
                assert_eq!(
                    collection.owner,
                    CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
                );
                Some(collection.zone)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_shuffled_zone_moves_into_the_library_before_one_shuffle() {
    for (text, zones) in [
        (
            "shuffle your graveyard into your library.",
            vec![CardZoneName::Graveyard],
        ),
        (
            "shuffle your hand and graveyard into your library, then draw seven cards.",
            vec![CardZoneName::Hand, CardZoneName::Graveyard],
        ),
        (
            "shuffle your hand into your library.",
            vec![CardZoneName::Hand],
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("shuffle-into", &tokens);
        let actions = try_recognize_shuffle_zones_into_library(&mut cursor)
            .unwrap()
            .expect("the frame claims its own verb");
        assert_eq!(zone_moves(&actions), zones, "{text}");
        assert_eq!(
            actions.last(),
            Some(&CardActionNode::Shuffle {
                owner: LocalPlayerReference::Controller
            }),
            "{text}"
        );
        assert_eq!(actions.len(), zones.len() + 1, "{text}");
        // The parent owns the connector or the sentence end.
        assert!(matches!(cursor.peek(), Some("." | ",")), "{text}");
    }
}

#[test]
fn shuffling_a_library_alone_and_other_objects_stay_with_their_readers() {
    for text in [
        "shuffle your library.",
        "shuffle.",
        "shuffle target artifact into its owner's library.",
        "shuffles their graveyard into their library.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("shuffle-into", &tokens);
        assert!(
            try_recognize_shuffle_zones_into_library(&mut cursor)
                .unwrap()
                .is_none(),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
}

#[test]
fn a_claimed_shuffle_fails_explicitly_on_an_unread_zone_or_destination() {
    for (text, state) in [
        (
            "shuffle your hand, graveyard, and all permanents you own into your library.",
            "zone-list.zone",
        ),
        (
            "shuffle your graveyard into target player's library.",
            "effect.shuffle-into.destination",
        ),
        (
            "shuffle your graveyard and library into your library.",
            "effect.shuffle-into.zone-kind",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("shuffle-into", &tokens);
        let failure = try_recognize_shuffle_zones_into_library(&mut cursor)
            .expect_err("the claimed frame owns its failure");
        assert_eq!(failure.state, state, "{text}");
    }
}
