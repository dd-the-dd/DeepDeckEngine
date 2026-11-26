//! Runtime proof for player performers and player relatives compiled from
//! real Oracle text: a zone holder named through the binding environment
//! (`the number of cards in that player's hand`, CR 400.1), a plural player
//! anaphor performing a clause per member (`Those players each discard two
//! cards at random`, CR 608.2c), and a population restricted by its
//! members' measured performance of the preceding population action
//! (`Each player who does draws seven cards`, `Each opponent who can't
//! loses 3 life`, CR 608.2c, 609.3).
use super::*;

/// Accepts every optional instruction, except for the players listed in
/// `declining`.
#[derive(Default)]
struct Choosing {
    declining: Vec<String>,
}
impl DecisionProvider for Choosing {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let declines = |option: &LegalAction| {
            option
                .decisions
                .values()
                .any(|value| *value == json!(["Decline"]))
        };
        if let Some(decline) = request.options.iter().position(declines)
            && request.options.len() == 2
        {
            let declining = self.declining.contains(&request.player_id);
            return Ok(if declining { decline } else { 1 - decline });
        }
        Ok(0)
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{1}".to_string(),
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
    let card = card_instance("player-0", &definition("source", "Sorcery"), 9999);
    let stack = StackObject {
        id: "player-performers".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn hold(engine: &mut GameEngine, player: usize, name: &str) {
    let index = 2000 + engine.state.players[player].hand.len() + player * 100;
    let card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), "Instant"),
        index,
    );
    engine.state.players[player].hand.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
}

fn bury(engine: &mut GameEngine, player: usize, name: &str) {
    let index = 3000 + engine.state.players[player].graveyard.len() + player * 100;
    let card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), "Sorcery"),
        index,
    );
    engine.state.players[player].graveyard.push(card);
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
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn run(
    engine: &mut GameEngine,
    stack: &StackObject,
    rule: &Value,
    target: Option<TargetRef>,
    provider: &mut Choosing,
) {
    let mut stack = stack.clone();
    // A resolving spell is the card itself on the stack (CR 608.2): no
    // ability kind or rule, so its damage source is the spell, never a
    // battlefield permanent whose incarnation must be authenticated.
    if rule["kind"] != "spellAbility" {
        stack.ability_rule = Some(rule.clone());
        stack.ability_kind = rule["kind"].as_str().map(str::to_string);
    }
    if let Some(target) = target {
        let id = rule["declaration"]["decisions"]
            .as_array()
            .and_then(|decisions| {
                decisions
                    .iter()
                    .find(|decision| decision["kind"] == "chooseTargets")
            })
            .and_then(|decision| decision["id"].as_str())
            .expect("target id");
        engine
            .capture_target_incarnation(&mut stack.decisions, id, &target)
            .expect("target designation is captured at declaration");
        stack.targets.insert(id.to_string(), target);
    }
    let effects = &rule["effects"];
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    let context = engine.target_numeric_context(&stack);
    engine
        .with_target_numeric_context(context, |engine| {
            engine.execute_effects(
                effects,
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                provider,
            )
        })
        .expect("the compiled program executes");
}

fn player(id: &str) -> Option<TargetRef> {
    Some(TargetRef::Player {
        player_id: id.to_string(),
    })
}

fn life(engine: &GameEngine) -> Vec<i32> {
    engine
        .state
        .players
        .iter()
        .map(|player| player.life)
        .collect()
}

#[test]
fn sudden_impact_counts_the_hand_of_the_player_it_targets() {
    let rule = compile(
        "Sudden Impact",
        "Instant",
        "Sudden Impact deals damage to target player equal to the number of cards in that player's hand.",
    );
    let (mut engine, stack) = position(3);
    for name in ["a", "b", "c"] {
        hold(&mut engine, 1, name);
    }
    // Another player's hand is not the holder.
    for name in ["d", "e", "f", "g", "h"] {
        hold(&mut engine, 2, name);
    }
    hold(&mut engine, 0, "mine");
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut Choosing::default(),
    );
    assert_eq!(life(&engine), [20, 17, 20]);
}

#[test]
fn their_graveyard_is_each_drawing_members_own_graveyard() {
    // Nature's Resurgence: `their` is the player of the action being read.
    let rule = compile(
        "Nature's Resurgence",
        "Sorcery",
        "Each player draws a card for each creature card in their graveyard.",
    );
    let (mut engine, stack) = position(2);
    let creature = |engine: &mut GameEngine, player: usize, name: &str| {
        let index = 4000 + engine.state.players[player].graveyard.len() + player * 100;
        let card = card_instance(
            &format!("player-{player}"),
            &definition(&format!("{name}-{player}"), "Creature — Bear"),
            index,
        );
        engine.state.players[player].graveyard.push(card);
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture admission");
    };
    creature(&mut engine, 0, "bear");
    creature(&mut engine, 1, "bear");
    creature(&mut engine, 1, "wolf");
    bury(&mut engine, 1, "spell");
    run(&mut engine, &stack, &rule, None, &mut Choosing::default());
    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert_eq!(engine.state.players[1].hand.len(), 2);
}

#[test]
fn skull_rend_makes_each_opponent_dealt_damage_discard_two_at_random() {
    let rule = compile(
        "Skull Rend",
        "Sorcery",
        "Skull Rend deals 2 damage to each opponent. Those players each discard two cards at random.",
    );
    let (mut engine, stack) = position(3);
    for player in 0..3 {
        for name in ["a", "b", "c"] {
            hold(&mut engine, player, name);
        }
    }
    run(&mut engine, &stack, &rule, None, &mut Choosing::default());
    assert_eq!(life(&engine), [20, 18, 18]);
    // Each opponent discards two of its own cards; the controller keeps its
    // hand (CR 701.9a).
    assert_eq!(engine.state.players[0].hand.len(), 3);
    for opponent in 1..3 {
        assert_eq!(engine.state.players[opponent].hand.len(), 1);
        assert_eq!(engine.state.players[opponent].graveyard.len(), 2);
        assert!(
            engine.state.players[opponent]
                .graveyard
                .iter()
                .all(|card| card.owner == format!("player-{opponent}"))
        );
    }
}

#[test]
fn each_opponent_who_cant_discard_loses_life() {
    // An unused combination: a quantified discard producer and an elided
    // `who can't` relative of the same population.
    let rule = compile(
        "Performance Probe",
        "Sorcery",
        "Each opponent discards a card. Each opponent who can't loses 3 life.",
    );
    let (mut engine, stack) = position(3);
    hold(&mut engine, 1, "kept");
    run(&mut engine, &stack, &rule, None, &mut Choosing::default());
    assert!(engine.state.players[1].hand.is_empty());
    assert_eq!(engine.state.players[1].graveyard.len(), 1);
    // The opponent with an empty hand could not discard (CR 609.3).
    assert_eq!(life(&engine), [20, 20, 17]);
}

#[test]
fn each_player_who_does_draws_only_for_the_players_who_shuffled() {
    // Step Between Worlds without its self-exile sentence, which the
    // runtime harness has no stack zone for.
    let rule = compile(
        "Shuffle Probe",
        "Sorcery",
        "Each player may shuffle their hand and graveyard into their library. Each player who does draws seven cards.",
    );
    let (mut engine, stack) = position(2);
    for player in 0..2 {
        hold(&mut engine, player, "held");
        bury(&mut engine, player, "buried");
    }
    let mut provider = Choosing {
        declining: vec!["player-1".to_string()],
    };
    run(&mut engine, &stack, &rule, None, &mut provider);
    // The accepting player shuffled both zones away and drew seven.
    assert_eq!(engine.state.players[0].hand.len(), 7);
    assert!(engine.state.players[0].graveyard.is_empty());
    assert_eq!(engine.state.players[0].library.len(), 14 - 7);
    // The declining player kept its zones and drew nothing.
    assert_eq!(engine.state.players[1].hand.len(), 1);
    assert_eq!(engine.state.players[1].graveyard.len(), 1);
    assert_eq!(engine.state.players[1].library.len(), 12);
}
