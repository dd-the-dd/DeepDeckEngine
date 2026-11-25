//! Runtime proofs of the costs whose objects the text names by place
//! (`discard your hand`, `exile the top N cards of your library`, `tap|sacrifice
//! enchanted creature`) and of a card placed beneath the top cards of a
//! library (`second from the top`, CR 401.2).
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

fn engine() -> GameEngine {
    let mut filler = definition("designated-filler", "Basic Land - Forest");
    filler.power = None;
    filler.toughness = None;
    GameEngine::new(
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
    .expect("designated cost test setup is valid")
}

fn activation(source: &str) -> LegalAction {
    LegalAction {
        id: format!("activate:{source}"),
        kind: ActionKind::ActivateAbility,
        player_id: "player-0".to_string(),
        label: "Activate test ability".to_string(),
        card_instance_id: Some(source.to_string()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
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

/// An Aura source attached to a creature of player 0, with a hand and a
/// library of known cards.
fn board() -> GameEngine {
    let mut engine = engine();
    let mut aura = card("aura", "Enchantment - Aura");
    aura.attached_to = Some("enchanted".to_string());
    engine.state.players[0].battlefield = vec![card("enchanted", "Creature - Bear"), aura];
    engine.state.players[0].hand = vec![card("hand-a", "Instant"), card("hand-b", "Sorcery")];
    engine.state.players[0].library = vec![
        card("bottom", "Instant"),
        card("middle", "Instant"),
        card("top", "Instant"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

#[test]
fn designated_cost_kinds_are_supported_costs() {
    for cost in [
        json!({ "kind": "discardHand", "player": { "kind": "abilityController" } }),
        json!({ "kind": "exileTopLibraryCards", "player": { "kind": "abilityController" }, "count": { "kind": "integer", "value": 2 } }),
        json!({ "kind": "tapAttachedPermanent" }),
        json!({ "kind": "sacrificeAttachedPermanent" }),
    ] {
        assert_eq!(chosen_object_cost_supported(&cost), Some(true), "{cost}");
    }
    assert_eq!(
        chosen_object_cost_supported(
            &json!({ "kind": "exileTopLibraryCards", "count": { "kind": "integer", "value": 0 } })
        ),
        Some(false)
    );
}

#[test]
fn discarding_the_hand_and_exiling_top_cards_pay_with_the_named_cards() {
    let mut engine = board();
    let rule = rule(json!([
        { "kind": "discardHand", "player": { "kind": "abilityController" } },
        { "kind": "exileTopLibraryCards", "player": { "kind": "abilityController" }, "count": { "kind": "integer", "value": 2 } },
    ]));
    assert!(engine.activated_chosen_object_costs_are_payable(&rule, "player-0", "aura"));
    engine
        .apply_activated_chosen_object_costs(&activation("aura"), &rule)
        .expect("the designated costs are paid");
    let player = &engine.state.players[0];
    assert!(player.hand.is_empty());
    let graveyard = player
        .graveyard
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(graveyard, BTreeSet::from(["hand-a", "hand-b"]));
    let exiled = player
        .exile
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(exiled, BTreeSet::from(["middle", "top"]));
    assert_eq!(
        player
            .library
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        vec!["bottom"]
    );
}

#[test]
fn a_library_shorter_than_the_exiled_top_cards_cannot_pay() {
    let engine = board();
    let rule = rule(json!([
        { "kind": "exileTopLibraryCards", "count": { "kind": "integer", "value": 4 } },
    ]));
    assert!(!engine.activated_chosen_object_costs_are_payable(&rule, "player-0", "aura"));
}

#[test]
fn tapping_the_enchanted_creature_pays_only_while_it_is_untapped() {
    let mut engine = board();
    let rule = rule(json!([{ "kind": "tapAttachedPermanent" }]));
    assert!(engine.activated_chosen_object_costs_are_payable(&rule, "player-0", "aura"));
    engine
        .apply_activated_chosen_object_costs(&activation("aura"), &rule)
        .expect("the enchanted creature is tapped");
    let (player, index) = engine
        .permanent_position("enchanted")
        .expect("the enchanted creature stays");
    assert!(engine.state.players[player].battlefield[index].tapped);
    assert!(!engine.activated_chosen_object_costs_are_payable(&rule, "player-0", "aura"));
}

#[test]
fn a_card_placed_second_from_the_top_has_one_card_above_it() {
    let mut engine = board();
    assert!(engine.place_library_card_beneath_top(0, "bottom", 1));
    assert_eq!(
        engine.state.players[0]
            .library
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        vec!["middle", "bottom", "top"]
    );
    // More cards above than the library holds: the bottom.
    assert!(engine.place_library_card_beneath_top(0, "top", 9));
    assert_eq!(engine.state.players[0].library[0].instance_id, "top");
    assert!(!engine.place_library_card_beneath_top(0, "absent", 1));
}
