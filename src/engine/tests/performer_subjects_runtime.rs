//! Runtime proof of the clause layer's player subjects, compiled from real
//! Oracle text: a player named through an earlier object (`that player`
//! after `its owner's hand`, the controller of a target an opponent
//! controls), the shuffle of zones into a library performed by every player
//! of a population (CR 701.24a, 608.2c), and extra turns (CR 500.7).
use super::*;

/// Accepts every optional instruction and otherwise records who chose what.
#[derive(Default)]
struct Accepting {
    decline: bool,
    players: Vec<String>,
    hand_total: Vec<usize>,
}
impl DecisionProvider for Accepting {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        self.hand_total
            .push(state.players.iter().map(|p| p.hand.len()).sum());
        let declines = |option: &LegalAction| {
            option
                .decisions
                .values()
                .any(|value| *value == json!(["Decline"]))
        };
        if let Some(decline) = request.options.iter().position(declines)
            && request.options.len() == 2
        {
            return Ok(if self.decline { decline } else { 1 - decline });
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
    let card = card_instance("player-0", &definition("source", "Creature"), 9999);
    let stack = StackObject {
        id: "subject-verb".to_string(),
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

fn add(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) -> String {
    let index = 1000 + engine.state.players[player].battlefield.len() + player * 100;
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    card.controller = format!("player-{player}");
    let id = card.instance_id.clone();
    engine.state.players[player].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    id
}

fn hold(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) {
    let index = 2000 + engine.state.players[player].hand.len() + player * 100;
    let card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    engine.state.players[player].hand.push(card);
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
    provider: &mut Accepting,
) {
    let mut stack = stack.clone();
    stack.ability_rule = Some(rule.clone());
    stack.ability_kind = rule["kind"].as_str().map(str::to_string);
    if let Some(target) = target {
        // Activation costs declare their own choices before the target.
        let id = rule["declaration"]["decisions"]
            .as_array()
            .and_then(|decisions| {
                decisions
                    .iter()
                    .find(|decision| decision["kind"] == "chooseTargets")
            })
            .and_then(|decision| decision["id"].as_str())
            .expect("target id");
        // Declaration captures the target's incarnation (CR 608.2b).
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

fn names(cards: &[CardInstance]) -> Vec<String> {
    cards
        .iter()
        .map(|card| card.definition.id.clone())
        .collect()
}

fn bury(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) {
    let index = 3000 + engine.state.players[player].graveyard.len() + player * 100;
    let card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    engine.state.players[player].graveyard.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
}

#[test]
fn recoil_makes_the_bounced_permanent_owner_discard() {
    let rule = compile(
        "Recoil",
        "Instant",
        "Return target permanent to its owner's hand. Then that player discards a card.",
    );
    // The permanent player 0 controls is owned by player 1: `that player`
    // is the owner its return named, not the controller.
    let (mut engine, stack) = position(2);
    let index = 1000 + engine.state.players[0].battlefield.len();
    let mut stolen = card_instance("player-1", &definition("bear-1", "Creature"), index);
    stolen.controller = "player-0".to_string();
    let stolen_id = stolen.instance_id.clone();
    engine.state.players[0].battlefield.push(stolen);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    hold(&mut engine, 0, "kept", "Instant");
    let mut provider = Accepting::default();
    run(
        &mut engine,
        &stack,
        &rule,
        Some(TargetRef::Permanent {
            instance_id: stolen_id.clone(),
        }),
        &mut provider,
    );
    assert!(engine.permanent_position(&stolen_id).is_none());
    // The bounced card was the owner's only card: it is the one discarded.
    assert!(engine.state.players[1].hand.is_empty());
    assert_eq!(names(&engine.state.players[1].graveyard), ["bear-1"]);
    assert_eq!(names(&engine.state.players[0].hand), ["kept-0"]);
    assert!(engine.state.players[0].graveyard.is_empty());
}

#[test]
fn that_player_after_a_target_an_opponent_controls_is_its_controller() {
    let rule = compile(
        "Suspended Sentence Probe",
        "Instant",
        "Destroy target creature an opponent controls. That player loses 3 life.",
    );
    let (mut engine, stack) = position(3);
    let bear = add(&mut engine, 2, "bear", "Creature");
    run(
        &mut engine,
        &stack,
        &rule,
        Some(TargetRef::Permanent {
            instance_id: bear.clone(),
        }),
        &mut Accepting::default(),
    );
    assert!(engine.permanent_position(&bear).is_none());
    assert_eq!(engine.state.players[2].life, 17);
    assert_eq!(engine.state.players[1].life, 20);
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn timetwister_shuffles_each_players_hand_and_graveyard_then_draws_seven() {
    let rule = compile(
        "Timetwister",
        "Sorcery",
        "Each player shuffles their hand and graveyard into their library, then draws seven cards. (Then put Timetwister into its owner's graveyard.)",
    );
    let (mut engine, stack) = position(2);
    let mut libraries = Vec::new();
    for seat in 0..2 {
        hold(&mut engine, seat, "held", "Instant");
        bury(&mut engine, seat, "buried", "Sorcery");
        bury(&mut engine, seat, "older", "Land");
        libraries.push(engine.state.players[seat].library.len());
    }
    run(&mut engine, &stack, &rule, None, &mut Accepting::default());
    for seat in 0..2 {
        let player = &engine.state.players[seat];
        assert!(player.graveyard.is_empty(), "seat {seat}");
        assert_eq!(player.hand.len(), 7, "seat {seat}");
        // Three cards went into the library before the seven were drawn.
        assert_eq!(player.library.len(), libraries[seat] + 3 - 7, "seat {seat}");
        // Every card stayed with its owner (CR 400.3).
        assert!(
            player
                .library
                .iter()
                .chain(player.hand.iter())
                .all(|card| card.owner == format!("player-{seat}")),
            "seat {seat}"
        );
    }
}

#[test]
fn a_bound_player_shuffles_only_their_own_graveyard() {
    let rule = compile(
        "Shuffle Probe",
        "Sorcery",
        "Target player mills three cards. That player shuffles their graveyard into their library.",
    );
    let (mut engine, stack) = position(2);
    bury(&mut engine, 0, "mine", "Instant");
    bury(&mut engine, 1, "theirs", "Instant");
    let library = engine.state.players[1].library.len();
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut Accepting::default(),
    );
    assert!(engine.state.players[1].graveyard.is_empty());
    assert_eq!(engine.state.players[1].library.len(), library + 1);
    assert_eq!(names(&engine.state.players[0].graveyard), ["mine-0"]);
}

#[test]
fn an_extra_turn_is_granted_to_the_player_who_takes_it() {
    for (text, target, granted) in [
        ("Take an extra turn after this one.", None, "player-0"),
        (
            "Target player takes an extra turn after this one.",
            player("player-1"),
            "player-1",
        ),
    ] {
        let rule = compile("Extra Turn Probe", "Sorcery", text);
        let (mut engine, stack) = position(2);
        run(
            &mut engine,
            &stack,
            &rule,
            target,
            &mut Accepting::default(),
        );
        let granted_turns = engine
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some("extraTurnForPlayer")
                    && modifier["playerId"].as_str() == Some(granted)
            })
            .count();
        assert_eq!(granted_turns, 1, "{text}");
        // The next turn is that player's (CR 500.7).
        engine.advance_to_next_turn_player();
        assert_eq!(
            engine.state.players[engine.state.active_player].id, granted,
            "{text}"
        );
    }
}
