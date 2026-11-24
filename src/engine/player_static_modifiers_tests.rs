//! Runtime proofs of the static frame's player predicates: each modifier
//! applies to its source-relative player population at the moment the rules
//! check it (casting, drawing, cleanup, untap step, targeting, activation).
use super::super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn opponents() -> Value {
    json!({ "kind": "opponentsOf", "player": controller() })
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": type_line.contains("Creature").then_some("2"),
        "toughness": type_line.contains("Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid")
}

fn instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2207,
    )
    .expect("player static modifier test engine is valid")
}

fn static_rule(modifiers: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": modifiers,
    })
}

/// An engine whose player-0 controls one source with `rule`.
fn engine_with_source(rule: Value) -> GameEngine {
    assert!(rule_is_executable(&rule), "rule is executable: {rule}");
    let mut engine = engine();
    let mut source = definition("static-source", "Enchantment");
    source.rules = vec![rule];
    engine.state.players[0]
        .battlefield
        .push(instance("static-source", source, "player-0"));
    engine
}

#[test]
fn spell_filtered_cast_prohibition_reads_the_card_and_its_population() {
    // `You can't cast creature spells.`
    let engine = engine_with_source(static_rule(json!([{
        "kind": "prohibitCast",
        "players": controller(),
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    }])));
    let creature = definition("bear", "Creature - Bear");
    let sorcery = definition("rite", "Sorcery");
    assert!(engine.card_casting_prohibited("player-0", &creature));
    assert!(!engine.card_casting_prohibited("player-0", &sorcery));
    assert!(!engine.card_casting_prohibited("player-1", &creature));
    // A filtered prohibition never prohibits every spell.
    assert!(!engine.spell_casting_prohibited("player-0"));
}

#[test]
fn combat_window_cast_prohibition_applies_only_during_combat() {
    // `Players can't cast spells during combat.`
    let mut engine = engine_with_source(static_rule(json!([{
        "kind": "prohibitCast",
        "players": { "kind": "eachPlayer" },
        "duringCombat": true,
    }])));
    engine.state.step = GameStep::PrecombatMain;
    assert!(!engine.spell_casting_prohibited("player-1"));
    engine.state.step = GameStep::DeclareBlockers;
    assert!(engine.spell_casting_prohibited("player-1"));
    assert!(engine.spell_casting_prohibited("player-0"));
}

#[test]
fn draw_limit_stops_further_draws_of_its_population_only() {
    // `Each opponent can't draw more than one card each turn.`
    let mut engine = engine_with_source(static_rule(json!([{
        "kind": "drawLimitPerTurn",
        "players": opponents(),
        "maximum": { "kind": "integer", "value": 1 },
    }])));
    let opponent_hand = engine.state.players[1].hand.len();
    let controller_hand = engine.state.players[0].hand.len();
    engine.draw_cards("player-1", 3).expect("draws resolve");
    engine.draw_cards("player-0", 2).expect("draws resolve");
    assert_eq!(engine.state.players[1].hand.len(), opponent_hand + 1);
    assert_eq!(engine.state.players[0].hand.len(), controller_hand + 2);
}

#[test]
fn player_keyword_and_life_gain_prohibition_follow_their_population() {
    // `Each opponent has hexproof.` and `Players can't gain life.`
    let engine = engine_with_source(static_rule(json!([
        { "kind": "playerHexproof", "player": opponents() },
        { "kind": "prohibitPlayerAction", "players": { "kind": "eachPlayer" }, "action": "gainLife" },
        { "kind": "prohibitPlayerAction", "players": controller(), "action": "playLand" },
    ])));
    assert!(engine.player_has_battlefield_static_modifier("player-1", "playerHexproof"));
    assert!(!engine.player_has_battlefield_static_modifier("player-0", "playerHexproof"));
    assert!(engine.player_action_prohibited("player-0", "gainLife"));
    assert!(engine.player_action_prohibited("player-1", "gainLife"));
    assert!(engine.player_action_prohibited("player-0", "playLand"));
    assert!(!engine.player_action_prohibited("player-1", "playLand"));
}

#[test]
fn hand_size_adjustments_apply_after_the_base_maximum() {
    // `Each opponent's maximum hand size is reduced by seven.` and `Your
    // maximum hand size is increased by one.`
    let engine = engine_with_source(static_rule(json!([
        { "kind": "modifyMaximumHandSize", "player": opponents(),
          "amount": { "kind": "integer", "value": -7 } },
        { "kind": "modifyMaximumHandSize", "player": controller(),
          "amount": { "kind": "integer", "value": 1 } },
    ])));
    assert_eq!(engine.static_adjusted_maximum_hand_size("player-1", 7), 0);
    assert_eq!(engine.static_adjusted_maximum_hand_size("player-0", 7), 8);
    assert_eq!(
        engine.static_adjusted_maximum_hand_size("player-0", usize::MAX),
        usize::MAX
    );
}

#[test]
fn attached_activation_prohibition_spares_mana_abilities_when_excepted() {
    // `Enchanted permanent ... its activated abilities can't be activated
    // unless they're mana abilities.`
    let rule = static_rule(json!([{
        "kind": "prohibitActivation",
        "abilities": {
            "kind": "activatedAbilities",
            "sources": { "kind": "attachedPermanent", "attachment": { "kind": "self" } },
            "where": { "kind": "not", "operand": { "kind": "isManaAbility" } },
        },
    }]));
    let mut engine = engine_with_source(rule);
    engine.state.players[1].battlefield.push(instance(
        "enchanted",
        definition("enchanted", "Artifact"),
        "player-1",
    ));
    engine.state.players[1].battlefield.push(instance(
        "other",
        definition("other", "Artifact"),
        "player-1",
    ));
    engine.state.players[0].battlefield[0].attached_to = Some("enchanted".to_string());
    let enchanted = engine.state.players[1].battlefield[0].clone();
    let other = engine.state.players[1].battlefield[1].clone();
    assert!(engine.activation_prohibited(&enchanted, false));
    assert!(!engine.activation_prohibited(&enchanted, true));
    assert!(!engine.activation_prohibited(&other, false));
}

#[test]
fn untap_limit_lets_the_active_player_untap_only_the_limit() {
    // `Players can't untap more than one creature during their untap steps.`
    let mut engine = engine_with_source(static_rule(json!([{
        "kind": "untapLimitPerUntapStep",
        "players": { "kind": "eachPlayer" },
        "objects": {
            "kind": "permanents",
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        },
        "maximum": { "kind": "integer", "value": 1 },
    }])));
    for id in ["bear-a", "bear-b"] {
        let mut creature = instance(id, definition(id, "Creature - Bear"), "player-1");
        creature.tapped = true;
        engine.state.players[1].battlefield.push(creature);
    }
    let mut land = instance("land", definition("land", "Land"), "player-1");
    land.tapped = true;
    engine.state.players[1].battlefield.push(land);
    let candidates = vec![
        "bear-a".to_string(),
        "bear-b".to_string(),
        "land".to_string(),
    ];
    let untapped = engine
        .apply_static_untap_limits(&mut FirstOption, "player-1", candidates)
        .expect("the untap limit resolves");
    assert_eq!(untapped.len(), 2);
    assert!(untapped.contains(&"land".to_string()));
    assert_eq!(
        untapped.iter().filter(|id| id.starts_with("bear")).count(),
        1
    );
}

#[test]
fn other_player_untap_collection_untaps_the_controllers_permanents() {
    // `Untap all permanents you control during each other player's untap step.`
    let mut engine = engine_with_source(static_rule(json!([{
        "kind": "untapDuringOtherPlayersUntap",
        "objects": { "kind": "permanents", "where": null, "controller": controller() },
    }])));
    let mut mine = instance("mine", definition("mine", "Artifact"), "player-0");
    mine.tapped = true;
    engine.state.players[0].battlefield.push(mine);
    let mut theirs = instance("theirs", definition("theirs", "Artifact"), "player-1");
    theirs.tapped = true;
    engine.state.players[1].battlefield.push(theirs);
    let ids = engine.static_other_player_untap_ids("player-1");
    assert_eq!(ids, vec!["mine".to_string()]);
    assert!(engine.static_other_player_untap_ids("player-0").is_empty());
}
