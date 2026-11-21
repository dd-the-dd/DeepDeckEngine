use super::*;
use deepdeck_engine_runtime::engine::{CardDefinition, GameEngine, GameSetup, PlayerDeck};

fn state() -> GameState {
    let card:CardDefinition=serde_json::from_value(serde_json::json!({"id":"filler","name":"Filler","typeLine":"Land","manaCost":"","isCommander":false,"isToken":false,"isGamePiece":false,"isSideboard":false,"power":null,"toughness":null,"rules":[]})).unwrap();
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("P{n}"),
                    starting_life: 20,
                    cards: vec![card.clone()],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        23,
    )
    .unwrap()
    .state()
    .clone()
}
#[test]
fn setup_identity_assertion_validates_presence_uniqueness_clock_and_does_not_repair() {
    let valid = state();
    assert!(evaluate_setup_object_identities(&valid, 2).0);
    assert!(!evaluate_setup_object_identities(&valid, 3).0);
    let mut missing = valid.clone();
    missing.players[0].library[0]
        .flags
        .retain(|key, _| !key.starts_with("trackedZoneObject:"));
    let mut duplicate = valid.clone();
    duplicate.players[1].library[0].flags = duplicate.players[0].library[0].flags.clone();
    let mut clock = valid.clone();
    clock.battlefield_incarnation_sequence = 0;
    for malformed in [missing, duplicate, clock] {
        let before = malformed.clone();
        assert!(!evaluate_setup_object_identities(&malformed, 2).0);
        assert_eq!(malformed, before);
    }
}
#[test]
fn setup_identity_fixture_assertion_deserializes_and_requires_count() {
    let value = serde_json::json!({"kind":"setupObjectIdentities","equals":2});
    assert!(matches!(
        serde_json::from_value::<crate::FixtureAssertion>(value).unwrap(),
        crate::FixtureAssertion::SetupObjectIdentities { equals: 2 }
    ));
    assert!(
        serde_json::from_value::<crate::FixtureAssertion>(
            serde_json::json!({"kind":"setupObjectIdentities"})
        )
        .is_err()
    );
}
