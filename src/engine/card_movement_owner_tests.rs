use super::super::*;
use super::PrivateZoneDestination;

#[test]
fn private_library_destinations_preserve_each_owners_subsequence() {
    let definition: CardDefinition =
        serde_json::from_value(json!({"id":"card","name":"card","typeLine":"Instant","rules":[]}))
            .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 2],
                })
                .collect(),
        },
        1944,
    )
    .unwrap();
    for destination in [
        PrivateZoneDestination::LibraryTop,
        PrivateZoneDestination::LibraryBottom,
    ] {
        for player in &mut engine.state.players {
            player.library.clear();
        }
        let cards = ["p0", "p1", "p0", "p1"]
            .into_iter()
            .enumerate()
            .map(|(i, owner)| card_instance(owner, &definition, i + 10))
            .collect::<Vec<_>>();
        let first_owner = vec![cards[0].instance_id.clone(), cards[2].instance_id.clone()];
        let second_owner = vec![cards[1].instance_id.clone(), cards[3].instance_id.clone()];
        let receipts = engine
            .put_cards_in_owners_private_zones(cards, destination)
            .unwrap();
        assert_eq!(
            receipts
                .iter()
                .map(|r| r.owner_id.as_str())
                .collect::<Vec<_>>(),
            vec!["p0", "p1", "p0", "p1"]
        );
        for (index, mut expected) in [first_owner, second_owner].into_iter().enumerate() {
            if matches!(destination, PrivateZoneDestination::LibraryTop) {
                expected.reverse();
            }
            assert_eq!(
                engine.state.players[index]
                    .library
                    .iter()
                    .map(|c| c.instance_id.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(
                engine.state.players[index]
                    .library
                    .iter()
                    .all(|c| c.owner == engine.state.players[index].id)
            );
        }
    }
}
