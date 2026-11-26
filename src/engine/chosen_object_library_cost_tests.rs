//! Runtime proofs of the costs that put cards into a library (CR 401.4,
//! 118.3): chosen cards from the payer's hand or graveyard (`Put a card from
//! your hand on top of your library`, `Put three cards from your graveyard
//! on the bottom of your library`) and the source card from the graveyard it
//! is activated in (`Put this card from your graveyard on the bottom of your
//! library`). The library's top card is its last element.
use super::super::*;
use super::chosen_object_cost_supported;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{1}".to_string(),
        name: id.to_string(),
        power: Some("2".to_string()),
        rules: Vec::new(),
        toughness: Some("2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn card(id: &str, type_line: &str) -> CardInstance {
    let mut card = card_instance("player-0", &definition(id, type_line), 0);
    card.instance_id = id.to_string();
    card
}

fn board() -> GameEngine {
    let mut filler = definition("library-cost-filler", "Basic Land - Forest");
    filler.power = None;
    filler.toughness = None;
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 4],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        512,
    )
    .expect("library cost test setup is valid");
    engine.state.players[0].battlefield = vec![card("source", "Artifact Creature - Dog")];
    engine.state.players[0].hand = vec![card("hand-a", "Instant"), card("hand-b", "Sorcery")];
    engine.state.players[0].graveyard = vec![
        card("grave-a", "Instant"),
        card("grave-b", "Creature - Bear"),
        card("grave-source", "Creature - Human"),
    ];
    engine.state.players[0].library = vec![card("bottom", "Instant"), card("top", "Instant")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn activation(source: &str, targets: &[(&str, &str)]) -> LegalAction {
    LegalAction {
        id: format!("activate:{source}"),
        kind: ActionKind::ActivateAbility,
        player_id: "player-0".to_string(),
        label: "Activate test ability".to_string(),
        card_instance_id: Some(source.to_string()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::new(),
        targets: targets
            .iter()
            .map(|(id, card)| {
                (
                    (*id).to_string(),
                    TargetRef::Card {
                        instance_id: (*card).to_string(),
                    },
                )
            })
            .collect(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    }
}

fn rule(costs: Value) -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": costs,
        "effects": [],
    })
}

fn library(engine: &GameEngine) -> Vec<&str> {
    engine.state.players[0]
        .library
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect()
}

#[test]
fn library_cost_kinds_are_supported_costs() {
    for cost in [
        json!({ "kind": "chosenObjectsCost", "action": "putOnLibraryTop", "choice": "activationCostLibrary0", "from": { "kind": "hand" } }),
        json!({ "kind": "chosenObjectsCost", "action": "putOnLibraryBottom", "choice": "activationCostLibrary0", "from": { "kind": "graveyard" } }),
        json!({ "kind": "putSourceInLibrary", "zone": "graveyard", "position": "bottom" }),
    ] {
        assert_eq!(chosen_object_cost_supported(&cost), Some(true), "{cost}");
    }
    // Only the graveyard source card has a payer here.
    assert_eq!(
        chosen_object_cost_supported(
            &json!({ "kind": "putSourceInLibrary", "zone": "battlefield", "position": "bottom" })
        ),
        Some(false)
    );
}

#[test]
fn a_chosen_hand_card_is_put_on_top_of_the_library() {
    let mut engine = board();
    let rule = rule(json!([{
        "kind": "chosenObjectsCost",
        "action": "putOnLibraryTop",
        "choice": "activationCostLibrary0",
        "from": { "kind": "hand", "player": { "kind": "abilityController" } },
    }]));
    engine
        .apply_activated_chosen_object_costs(
            &activation("source", &[("activationCostLibrary0", "hand-b")]),
            &rule,
        )
        .expect("the library cost is paid");
    assert_eq!(library(&engine), vec!["bottom", "top", "hand-b"]);
    assert_eq!(engine.state.players[0].hand.len(), 1);
}

#[test]
fn chosen_graveyard_cards_are_put_on_the_bottom_of_the_library() {
    let mut engine = board();
    let rule = rule(json!([{
        "kind": "chosenObjectsCost",
        "action": "putOnLibraryBottom",
        "choice": "activationCostLibrary0",
        "from": { "kind": "graveyard", "player": { "kind": "abilityController" } },
    }]));
    engine
        .apply_activated_chosen_object_costs(
            &activation(
                "source",
                &[
                    ("activationCostLibrary0:0", "grave-a"),
                    ("activationCostLibrary0:1", "grave-b"),
                ],
            ),
            &rule,
        )
        .expect("the library cost is paid");
    let library = library(&engine);
    assert_eq!(library.len(), 4);
    assert_eq!(&library[2..], ["bottom", "top"]);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
}

#[test]
fn the_graveyard_source_card_is_put_on_the_bottom_of_its_library() {
    let mut engine = board();
    let rule = rule(json!([
        { "kind": "putSourceInLibrary", "zone": "graveyard", "position": "bottom" },
    ]));
    engine
        .apply_activated_chosen_object_costs(&activation("grave-source", &[]), &rule)
        .expect("the source cost is paid");
    assert_eq!(library(&engine), vec!["grave-source", "bottom", "top"]);
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .all(|card| card.instance_id != "grave-source")
    );
}

#[test]
fn a_card_that_left_its_zone_cannot_pay_the_library_cost() {
    let mut engine = board();
    let rule = rule(json!([{
        "kind": "chosenObjectsCost",
        "action": "putOnLibraryTop",
        "choice": "activationCostLibrary0",
        "from": { "kind": "hand", "player": { "kind": "abilityController" } },
    }]));
    // `top` is in the library, not in the payer's hand or graveyard.
    assert!(
        engine
            .apply_activated_chosen_object_costs(
                &activation("source", &[("activationCostLibrary0", "top")]),
                &rule,
            )
            .is_err()
    );
}
