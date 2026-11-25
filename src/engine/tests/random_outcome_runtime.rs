//! Runtime proof of the random and contested outcome primitives the verb
//! lexicon lowers (`flip`, `roll`, `clash`, CR 705, 706, 701.30) and of
//! their definite antecedents (`the flip`, `the result`, `you win`), compiled
//! from real Oracle text where a card states them. Outcomes come from the
//! engine's seeded generator: the same seed replays the same outcomes.
use super::*;

/// Picks a fixed option index for every decision and records the players
/// asked.
struct FixedChoice {
    option: usize,
    players: Vec<String>,
}

impl FixedChoice {
    fn new(option: usize) -> Self {
        Self {
            option,
            players: Vec::new(),
        }
    }
}

impl DecisionProvider for FixedChoice {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        Ok(self.option.min(request.options.len().saturating_sub(1)))
    }
}

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: mana_cost.to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn position(seed: u64) -> (GameEngine, StackObject) {
    let engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", "{1}"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        seed,
    )
    .unwrap();
    let card = card_instance(
        "player-0",
        &definition("outcome-source", "Artifact", "{1}"),
        9999,
    );
    let stack = StackObject {
        id: "random-outcome".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn run_effects(
    engine: &mut GameEngine,
    stack: &StackObject,
    effects: &Value,
    provider: &mut FixedChoice,
) -> BTreeMap<String, RuntimeBinding> {
    assert!(
        effects.as_array().unwrap().iter().all(effect_supported),
        "{effects:#}"
    );
    let mut bindings = BTreeMap::new();
    engine
        .execute_effects(
            effects,
            stack,
            &mut bindings,
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the program resolves");
    bindings
}

fn integer_binding(bindings: &BTreeMap<String, RuntimeBinding>, id: &str) -> i32 {
    match bindings.get(id) {
        Some(RuntimeBinding::Integer(value)) => *value,
        other => panic!("{id} is not an integer binding: {other:?}"),
    }
}

/// Compiles real Oracle text with the production compiler; the runtime proof
/// executes exactly the IR it emits.
fn compile(name: &str, type_line: &str, oracle_text: &str, ability: usize) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[ability]
        .rule
        .clone()
        .expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn events<'a>(engine: &'a GameEngine, kind: &'a str) -> impl Iterator<Item = &'a GameEvent> {
    engine
        .state
        .events
        .iter()
        .filter(move |event| event.kind == kind)
}

#[test]
fn a_seeded_coin_flip_replays_identically_and_records_its_totals() {
    let flips = json!([{
        "kind": "flipCoins", "player": controller(),
        "count": { "kind": "integer", "value": 24 },
        "bind": "randomOutcome:coinFlip",
    }]);
    let outcomes = |seed: u64| {
        let (mut engine, stack) = position(seed);
        let bindings = run_effects(&mut engine, &stack, &flips, &mut FixedChoice::new(0));
        let results = events(&engine, "coinFlipped")
            .map(|event| event.detail["result"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        (results, bindings)
    };
    let (first, bindings) = outcomes(41);
    let (replayed, _) = outcomes(41);
    assert_eq!(first.len(), 24);
    assert_eq!(first, replayed, "the same seed replays the same flips");
    let heads = first.iter().filter(|result| *result == "heads").count() as i32;
    assert_eq!(
        integer_binding(&bindings, "randomOutcome:coinFlip:heads"),
        heads
    );
    assert_eq!(
        integer_binding(&bindings, "randomOutcome:coinFlip:tails"),
        24 - heads
    );
    // The call is heads (CR 705.2): a flip is won exactly when it is heads.
    assert_eq!(integer_binding(&bindings, "randomOutcome:coinFlip"), heads);
    assert_eq!(
        integer_binding(&bindings, "randomOutcome:coinFlip:lost"),
        24 - heads
    );
    // Over 24 fair flips both faces occur for this seed; a constant
    // generator would fail here.
    assert!(heads > 0 && heads < 24, "{first:?}");
}

#[test]
fn dice_results_stay_within_their_faces_and_the_result_is_their_total() {
    let (mut engine, stack) = position(7);
    let dice = json!([{
        "kind": "rollDice", "player": controller(),
        "count": { "kind": "integer", "value": 3 }, "sides": 6,
        "bind": "randomOutcome:dieRoll",
    }]);
    let bindings = run_effects(&mut engine, &stack, &dice, &mut FixedChoice::new(0));
    let results = events(&engine, "dieRolled")
        .map(|event| event.detail["result"].as_i64().unwrap() as i32)
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| (1..=6).contains(result)));
    assert_eq!(
        integer_binding(&bindings, "randomOutcome:dieRoll"),
        results.iter().sum::<i32>()
    );
    for (index, result) in results.iter().enumerate() {
        assert_eq!(
            integer_binding(&bindings, &format!("randomOutcome:dieRoll:{}", index + 1)),
            *result
        );
    }
    // A die needs two faces; a malformed die is refused before it runs.
    assert!(!effect_supported(&json!({
        "kind": "rollDice", "player": controller(),
        "count": { "kind": "integer", "value": 1 }, "sides": 1,
        "bind": "randomOutcome:dieRoll",
    })));
}

#[test]
fn the_win_and_lose_branches_follow_the_flip_compiled_from_oracle_text() {
    // Puppet's Verdict's shape over observable life and card counts.
    let rule = compile(
        "Flip Witness",
        "Sorcery",
        "Flip a coin. If you win the flip, draw a card. If you lose the flip, you lose 3 life.",
        0,
    );
    for seed in 0..16 {
        let (mut engine, stack) = position(seed);
        engine.state.players[0].hand.clear();
        run_effects(
            &mut engine,
            &stack,
            &rule["effects"],
            &mut FixedChoice::new(0),
        );
        let won = events(&engine, "coinFlipped")
            .next()
            .and_then(|event| event.detail["won"].as_bool())
            .expect("one flip");
        assert_eq!(engine.state.players[0].hand.len(), usize::from(won));
        assert_eq!(engine.state.players[0].life, if won { 20 } else { 17 });
    }
}

#[test]
fn the_result_selects_the_die_branch_and_restates_the_roll() {
    // Dissatisfied Customer's roll: `If the result is 3 or less, you lose
    // that much life.`
    let rule = compile(
        "Dissatisfied Customer",
        "Creature — Vampire Guest",
        "Flying, haste\nWhen this creature enters, roll a six-sided die. If the result is 3 or less, you lose that much life.",
        1,
    );
    let mut seen = BTreeSet::new();
    for seed in 0..24 {
        let (mut engine, stack) = position(seed);
        run_effects(
            &mut engine,
            &stack,
            &rule["effects"],
            &mut FixedChoice::new(0),
        );
        let result = events(&engine, "dieRolled")
            .next()
            .and_then(|event| event.detail["result"].as_i64())
            .expect("one roll") as i32;
        seen.insert(result <= 3);
        let expected = if result <= 3 { 20 - result } else { 20 };
        assert_eq!(engine.state.players[0].life, expected, "rolled {result}");
    }
    assert_eq!(seen.len(), 2, "both branches were taken over the seeds");
}

#[test]
fn a_d20_threshold_picks_the_right_branch() {
    let rule = compile(
        "Threshold Witness",
        "Sorcery",
        "Roll a d20. If you roll a 10 or higher, draw a card. Otherwise, you lose 1 life.",
        0,
    );
    for seed in 0..24 {
        let (mut engine, stack) = position(seed);
        engine.state.players[0].hand.clear();
        run_effects(
            &mut engine,
            &stack,
            &rule["effects"],
            &mut FixedChoice::new(0),
        );
        let result = events(&engine, "dieRolled")
            .next()
            .and_then(|event| event.detail["result"].as_i64())
            .expect("one roll");
        assert!((1..=20).contains(&result));
        let high = result >= 10;
        assert_eq!(engine.state.players[0].hand.len(), usize::from(high));
        assert_eq!(engine.state.players[0].life, if high { 20 } else { 19 });
    }
}

#[test]
fn a_clash_reveals_compares_and_lets_each_player_bottom_their_card() {
    let rule = compile(
        "Oaken Brawler",
        "Creature — Treefolk Warrior",
        "When this creature enters, clash with an opponent. If you win, you gain 2 life. (Each clashing player reveals the top card of their library, then puts that card on their choice of the top or bottom. A player wins if their card had a greater mana value.)",
        0,
    );
    for (mine, theirs, option, won) in [
        ("{3}", "{1}", 0, true),
        ("{1}", "{3}", 0, false),
        // A tie has no winner (CR 701.30d).
        ("{2}", "{2}", 1, false),
    ] {
        let (mut engine, stack) = position(3);
        engine.state.players[0].library.push(card_instance(
            "player-0",
            &definition("my-top", "Creature", mine),
            5000,
        ));
        engine.state.players[1].library.push(card_instance(
            "player-1",
            &definition("their-top", "Creature", theirs),
            5100,
        ));
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture admission");
        let mut provider = FixedChoice::new(option);
        let bindings = run_effects(&mut engine, &stack, &rule["effects"], &mut provider);
        assert_eq!(
            integer_binding(&bindings, "randomOutcome:clash"),
            i32::from(won)
        );
        assert_eq!(engine.state.players[0].life, if won { 22 } else { 20 });
        // Both clashing players chose where their card goes, in APNAP order.
        assert_eq!(provider.players, ["player-0", "player-1"]);
        let top_or_bottom = |player: usize, id: &str| {
            let library = &engine.state.players[player].library;
            if library.last().is_some_and(|card| card.instance_id == id) {
                "top"
            } else if library.first().is_some_and(|card| card.instance_id == id) {
                "bottom"
            } else {
                "elsewhere"
            }
        };
        let expected = if option == 0 { "top" } else { "bottom" };
        let my_id =
            card_instance("player-0", &definition("my-top", "Creature", mine), 5000).instance_id;
        let their_id = card_instance(
            "player-1",
            &definition("their-top", "Creature", theirs),
            5100,
        )
        .instance_id;
        assert_eq!(top_or_bottom(0, &my_id), expected);
        assert_eq!(top_or_bottom(1, &their_id), expected);
    }
}

#[test]
fn a_performer_flips_its_own_coin() {
    // `Each opponent flips a coin` is the opponents' instruction: the
    // performer slot runs the same primitive as that player's (CR 608.2c).
    let rule = compile(
        "Performer Witness",
        "Sorcery",
        "Each opponent flips a coin.",
        0,
    );
    let (mut engine, stack) = position(5);
    run_effects(
        &mut engine,
        &stack,
        &rule["effects"],
        &mut FixedChoice::new(0),
    );
    // Each performer's outcome stays in that player's iteration scope; the
    // published event names the flipping player.
    let flippers = events(&engine, "coinFlipped")
        .map(|event| event.player_id.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(flippers, ["player-1"]);
}

#[test]
fn recruit_draws_discards_and_creates_a_soldier_for_a_nonland_discard() {
    let rule = compile(
        "Lake-town Lookout",
        "Creature — Human Scout",
        "When this creature dies, recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)",
        0,
    );
    for (drawn_type, token) in [("Sorcery", true), ("Land", false)] {
        let (mut engine, stack) = position(9);
        engine.state.players[0].hand.clear();
        engine.state.players[0].library = vec![card_instance(
            "player-0",
            &definition("recruited", drawn_type, "{1}"),
            6000,
        )];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture admission");
        run_effects(
            &mut engine,
            &stack,
            &rule["effects"],
            &mut FixedChoice::new(0),
        );
        assert!(engine.state.players[0].hand.is_empty());
        assert_eq!(engine.state.players[0].graveyard.len(), 1);
        let soldiers = engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| {
                card.definition.is_token
                    && type_line_contains(&card.definition.type_line, "Soldier")
            })
            .count();
        assert_eq!(soldiers, usize::from(token), "{drawn_type}");
    }
}
