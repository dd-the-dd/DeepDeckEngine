//! Runtime proof of the programs lowered for one player subject with several
//! actions: a singular `target player sacrifices ...` (choose, then sacrifice,
//! for the targeted player only) and `each player sacrifices a creature,
//! discards a card, and loses 4 life` (each action completes for every player
//! before the next begins, CR 608.2c / 101.4), plus life, draw and mill for
//! wider populations. The IR below is exactly what the Oracle compiler emits.
use super::*;

#[derive(Default)]
struct FirstChoice {
    players: Vec<String>,
    battlefield_total: Vec<usize>,
    hand_total: Vec<usize>,
}
impl DecisionProvider for FirstChoice {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        self.battlefield_total
            .push(state.players.iter().map(|p| p.battlefield.len()).sum());
        self.hand_total
            .push(state.players.iter().map(|p| p.hand.len()).sum());
        Ok(0)
    }
}

fn creature() -> Value {
    json!({"kind":"cardTypeContains","value":"Creature"})
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn position(players: usize) -> (GameEngine, StackObject) {
    let engine = GameEngine::new(
        GameSetup {
            players: (0..players)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2024,
    )
    .unwrap();
    let card = card_instance("player-0", &definition("source", "Creature"), 9999);
    let stack = StackObject {
        id: "player-action".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn add(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) {
    let index = 1000 + engine.state.players[player].battlefield.len() + player * 100;
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    card.controller = format!("player-{player}");
    engine.state.players[player].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
}

fn give_hand(engine: &mut GameEngine, player: usize, cards: usize) {
    for n in 0..cards {
        let card = card_instance(
            &format!("player-{player}"),
            &definition(&format!("held-{player}-{n}"), "Instant"),
            2000 + player * 100 + n,
        );
        engine.state.players[player].hand.push(card);
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
}

/// Compiles real Oracle text with the production compiler; the runtime proof
/// executes exactly the IR it emits.
fn compile(name: &str, type_line: &str, oracle_text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule));
    rule
}

fn stack_for(stack: &StackObject, rule: &Value, target: Option<&str>) -> StackObject {
    let mut stack = stack.clone();
    stack.ability_rule = Some(rule.clone());
    stack.ability_kind = rule["kind"].as_str().map(str::to_string);
    if let Some(player) = target {
        let id = rule["declaration"]["decisions"][0]["id"]
            .as_str()
            .expect("player target id");
        stack.targets.insert(
            id.to_string(),
            TargetRef::Player {
                player_id: player.to_string(),
            },
        );
    }
    stack
}

fn run(
    engine: &mut GameEngine,
    stack: &StackObject,
    rule: &Value,
    target: Option<&str>,
) -> FirstChoice {
    let stack = stack_for(stack, rule, target);
    let effects = &rule["effects"];
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    let mut provider = FirstChoice::default();
    // Resolution snapshots target legality once, before any effect runs.
    let context = engine.target_numeric_context(&stack);
    engine
        .with_target_numeric_context(context, |engine| {
            engine.execute_effects(
                effects,
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut provider,
            )
        })
        .expect("the compiled program executes");
    provider
}

fn counts(engine: &GameEngine) -> Vec<(usize, usize)> {
    engine
        .state
        .players
        .iter()
        .map(|p| (p.battlefield.len(), p.graveyard.len()))
        .collect()
}

fn lives(engine: &GameEngine) -> Vec<i32> {
    engine.state.players.iter().map(|p| p.life).collect()
}

fn hands(engine: &GameEngine) -> Vec<usize> {
    engine.state.players.iter().map(|p| p.hand.len()).collect()
}

const EDICT: &str = "Target player sacrifices a creature.";

#[test]
fn real_edict_makes_only_the_targeted_player_choose_and_sacrifice() {
    let rule = compile("Diabolic Edict", "Instant", EDICT);
    let (mut engine, stack) = position(3);
    add(&mut engine, 0, "mine", "Creature");
    add(&mut engine, 1, "bear", "Creature");
    add(&mut engine, 1, "ogre", "Creature");
    add(&mut engine, 1, "relic", "Artifact");
    add(&mut engine, 2, "other", "Creature");
    let provider = run(&mut engine, &stack, &rule, Some("player-1"));
    // The targeted player is the chooser; nobody else is asked or affected.
    assert_eq!(provider.players, ["player-1".to_string()]);
    assert_eq!(counts(&engine), [(1, 0), (2, 1), (1, 0)]);
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.definition.id == "relic-1")
    );
}

#[test]
fn targeted_player_sacrifices_then_loses_life_even_without_a_creature() {
    let rule = compile(
        "Probe",
        "Creature",
        "When this creature enters, target player sacrifices a creature and loses 1 life.",
    );
    let (mut engine, stack) = position(2);
    add(&mut engine, 0, "mine", "Creature");
    add(&mut engine, 1, "relic", "Artifact");
    run(&mut engine, &stack, &rule, Some("player-1"));
    assert_eq!(counts(&engine), [(1, 0), (1, 0)]);
    // The coordinated life loss is an independent action and still applies.
    assert_eq!(lives(&engine), [20, 19]);

    let (mut engine, stack) = position(2);
    add(&mut engine, 1, "bear", "Creature");
    run(&mut engine, &stack, &rule, Some("player-1"));
    assert_eq!(counts(&engine), [(0, 0), (0, 1)]);
    assert_eq!(lives(&engine), [20, 19]);
}

#[test]
fn each_action_completes_for_every_player_before_the_next_begins() {
    let rule = compile(
        "Probe",
        "Creature",
        "When this creature dies, each player sacrifices a creature, discards a card, and loses 4 life.",
    );
    let (mut engine, stack) = position(3);
    for player in 0..3 {
        add(&mut engine, player, "bear", "Creature");
        add(&mut engine, player, "ogre", "Creature");
        give_hand(&mut engine, player, 2);
    }
    let before: usize = engine
        .state
        .players
        .iter()
        .map(|p| p.battlefield.len())
        .sum();
    let provider = run(&mut engine, &stack, &rule, None);
    assert_eq!(
        provider.players.len(),
        6,
        "three sacrifice choices, then three discard choices"
    );
    // Every sacrifice choice sees the untouched battlefield; every discard
    // choice sees the finished sacrifices (the action order is preserved).
    assert!(provider.battlefield_total[..3].iter().all(|n| *n == before));
    assert!(
        provider.battlefield_total[3..]
            .iter()
            .all(|n| *n == before - 3)
    );
    assert!(provider.hand_total[..3].iter().all(|n| *n == 6));
    // One sacrificed creature and one discarded card in each graveyard.
    assert_eq!(counts(&engine), [(1, 2), (1, 2), (1, 2)]);
    assert_eq!(hands(&engine), [1, 1, 1]);
    assert_eq!(lives(&engine), [16, 16, 16]);
}

#[test]
fn each_player_and_each_other_player_life_draw_and_mill_actions() {
    let each_player = compile(
        "Probe",
        "Creature",
        "When this creature dies, each player loses 2 life and draws a card.",
    );
    let others = compile(
        "Probe",
        "Creature",
        "When this creature dies, each other player gains 3 life, mills a card, and discards a card.",
    );
    let (mut engine, stack) = position(3);
    let libraries: Vec<usize> = engine
        .state
        .players
        .iter()
        .map(|p| p.library.len())
        .collect();
    run(&mut engine, &stack, &each_player, None);
    assert_eq!(lives(&engine), [18, 18, 18]);
    assert_eq!(hands(&engine), [1, 1, 1]);
    run(&mut engine, &stack, &others, None);
    // Only the controller's opponents gain, mill and then discard.
    assert_eq!(lives(&engine), [18, 21, 21]);
    assert_eq!(hands(&engine), [1, 0, 0]);
    assert_eq!(
        engine
            .state
            .players
            .iter()
            .map(|p| p.library.len())
            .collect::<Vec<_>>(),
        [libraries[0] - 1, libraries[1] - 2, libraries[2] - 2]
    );
}
