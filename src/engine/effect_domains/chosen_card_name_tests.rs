//! Runtime proof of a card name chosen while an effect resolves (CR 608.2d,
//! 201.2): the recorded name is what the shared `nameEquals` filter reads,
//! and a reference whose producer has not run is never guessed.
use super::super::super::*;

fn engine() -> GameEngine {
    let filler = CardDefinition {
        id: "name-filler".to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: String::new(),
        name: "Forest".to_string(),
        power: None,
        rules: Vec::new(),
        toughness: None,
        type_line: "Basic Land - Forest".to_string(),
    };
    GameEngine::new(
        GameSetup {
            opening_hand_size: 7,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 12],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        512,
    )
    .expect("card-name test setup is valid")
}

fn chosen_name() -> Value {
    json!({ "kind": "chosenCardName", "decisionId": "resolutionCardName" })
}

#[test]
fn a_recorded_card_name_becomes_the_shared_name_filter() {
    let engine = engine();
    let effect = json!({
        "kind": "discardCards",
        "where": { "kind": "and", "operands": [chosen_name(), { "kind": "not", "operand": {
            "kind": "cardTypeContains", "value": "Land" } }] },
    });
    let decisions = BTreeMap::from([("resolutionCardName".to_string(), json!(["Lightning Bolt"]))]);
    let materialized = engine.materialize_resolution_choices(&effect, &decisions);
    assert_eq!(
        materialized["where"]["operands"][0],
        json!({ "kind": "nameEquals", "value": "Lightning Bolt" })
    );
    // The rest of the predicate is untouched.
    assert_eq!(
        materialized["where"]["operands"][1],
        effect["where"]["operands"][1]
    );
}

#[test]
fn an_unproduced_card_name_is_never_guessed() {
    let engine = engine();
    let effect = json!({ "kind": "discardCards", "where": chosen_name() });
    let unchanged = engine.materialize_resolution_choices(&effect, &BTreeMap::new());
    assert_eq!(unchanged.as_ref(), &effect);
    // The unmaterialized atom matches no card: it is not `nameEquals`.
    let card = CardDefinition {
        id: "bolt".to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{R}".to_string(),
        name: "Lightning Bolt".to_string(),
        power: None,
        rules: Vec::new(),
        toughness: None,
        type_line: "Instant".to_string(),
    };
    assert!(!matches_card_filter(&card, &chosen_name()));
    assert!(matches_card_filter(
        &card,
        &json!({ "kind": "nameEquals", "value": "Lightning Bolt" })
    ));
}

#[test]
fn the_chosen_name_atom_is_an_admitted_card_filter() {
    assert!(card_filter_supported(&chosen_name()));
    assert!(!card_filter_supported(&json!({ "kind": "chosenCardName" })));
}
