//! Runtime proof for verbs after an explicit subject, compiled from real Oracle
//! text: the causative `have SUBJECT VERB` (the subject performs the ordinary
//! action), whole-hand player verbs (`reveals|discards their hand`, CR 701.20a
//! and 701.9a), the controller's choice from a revealed hand, a player's verb
//! on the chosen card, and a quantified `, then` list (CR 608.2c).
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

#[test]
fn castigate_exiles_only_a_card_matching_the_choice_from_the_revealed_hand() {
    let rule = compile(
        "Castigate",
        "Sorcery",
        "Target opponent reveals their hand. You choose a nonland card from it and exile that card.",
    );
    let (mut engine, stack) = position(2);
    hold(&mut engine, 1, "island", "Land");
    hold(&mut engine, 1, "shock", "Instant");
    let mut provider = Accepting::default();
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut provider,
    );
    // The controller chooses; the land is never a candidate.
    assert!(provider.players.iter().all(|id| id == "player-0"));
    assert_eq!(names(&engine.state.players[1].hand), ["island-1"]);
    assert_eq!(names(&engine.state.players[1].exile), ["shock-1"]);
    assert!(engine.state.players[1].graveyard.is_empty());
}

#[test]
fn activated_reveal_makes_the_target_discard_the_chosen_card() {
    let rule = compile(
        "Corpse Traders",
        "Creature — Human Rogue",
        "{2}{B}, Sacrifice a creature: Target opponent reveals their hand. You choose a card from it. That player discards that card. Activate only as a sorcery.",
    );
    let (mut engine, stack) = position(2);
    hold(&mut engine, 1, "bolt", "Instant");
    let mut provider = Accepting::default();
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut provider,
    );
    assert!(engine.state.players[1].hand.is_empty());
    assert_eq!(names(&engine.state.players[1].graveyard), ["bolt-1"]);

    // An empty revealed hand offers no choice and discards nothing.
    let (mut engine, stack) = position(2);
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut provider,
    );
    assert!(engine.state.players[1].graveyard.is_empty());
}

#[test]
fn causative_sacrifice_is_performed_by_the_targeted_opponent_when_accepted() {
    let rule = compile(
        "Predatory Nightstalker",
        "Creature — Vampire Assassin",
        "When this creature enters, you may have target opponent sacrifice a creature of their choice.",
    );
    let (mut engine, stack) = position(2);
    add(&mut engine, 0, "mine", "Creature");
    add(&mut engine, 1, "bear", "Creature");
    add(&mut engine, 1, "ogre", "Creature");
    let mut provider = Accepting::default();
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut provider,
    );
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
    assert_eq!(engine.state.players[1].graveyard.len(), 1);
    // The opponent, not the controller, chooses the sacrificed creature.
    assert_eq!(provider.players, ["player-0", "player-1"]);

    let (mut engine, stack) = position(2);
    add(&mut engine, 1, "bear", "Creature");
    let mut provider = Accepting {
        decline: true,
        ..Accepting::default()
    };
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut provider,
    );
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
}

#[test]
fn causative_player_set_reads_the_shared_life_clause_with_a_bare_verb() {
    // Unused combination, no dedicated code: causative `have` + a player
    // set subject + the shared life clause (bare verb from the causative).
    let rule = compile(
        "Causative Drain Probe",
        "Creature — Horror",
        "When this creature enters, you may have each opponent lose 2 life.",
    );
    let (mut engine, stack) = position(3);
    run(&mut engine, &stack, &rule, None, &mut Accepting::default());
    let life = |engine: &GameEngine| {
        engine
            .state
            .players
            .iter()
            .map(|player| player.life)
            .collect::<Vec<_>>()
    };
    assert_eq!(life(&engine), [20, 18, 18]);

    let (mut engine, stack) = position(3);
    let mut provider = Accepting {
        decline: true,
        ..Accepting::default()
    };
    run(&mut engine, &stack, &rule, None, &mut provider);
    assert_eq!(life(&engine), [20, 20, 20]);
}

#[test]
fn causative_damage_and_stat_change_use_the_ordinary_frames() {
    let rule = compile(
        "Hissing Iguanar",
        "Creature — Lizard",
        "Whenever another creature dies, you may have this creature deal 1 damage to target player or planeswalker.",
    );
    let (mut engine, stack) = position(2);
    run(
        &mut engine,
        &stack,
        &rule,
        player("player-1"),
        &mut Accepting::default(),
    );
    assert_eq!(engine.state.players[1].life, 19);

    let rule = compile(
        "Battle-Rattle Shaman",
        "Creature — Goblin Shaman",
        "At the beginning of combat on your turn, you may have target creature get +2/+0 until end of turn.",
    );
    let (mut engine, stack) = position(2);
    let bear = add(&mut engine, 1, "bear", "Creature");
    run(
        &mut engine,
        &stack,
        &rule,
        Some(TargetRef::Permanent {
            instance_id: bear.clone(),
        }),
        &mut Accepting::default(),
    );
    let (index, card) = engine.permanent_position(&bear).expect("bear");
    let power = engine.current_power(&engine.state.players[index].battlefield[card]);
    assert_eq!(power, 4);
}

#[test]
fn each_player_discards_their_hand_then_draws_seven() {
    let rule = compile(
        "Wheel of Fortune",
        "Sorcery",
        "Each player discards their hand, then draws seven cards.",
    );
    let (mut engine, stack) = position(2);
    for seat in 0..2 {
        hold(&mut engine, seat, "old", "Instant");
        hold(&mut engine, seat, "older", "Land");
    }
    run(&mut engine, &stack, &rule, None, &mut Accepting::default());
    for seat in 0..2 {
        assert_eq!(engine.state.players[seat].hand.len(), 7);
        assert_eq!(engine.state.players[seat].graveyard.len(), 2);
    }
}

#[test]
fn then_list_completes_every_draw_before_the_first_discard() {
    let rule = compile(
        "Lore Broker",
        "Creature — Human Rogue",
        "{T}: Each player draws a card, then discards a card.",
    );
    let (mut engine, stack) = position(2);
    for seat in 0..2 {
        hold(&mut engine, seat, "kept", "Instant");
    }
    let mut provider = Accepting::default();
    run(&mut engine, &stack, &rule, None, &mut provider);
    // The first discard choice already sees both drawn cards (CR 608.2c).
    assert_eq!(provider.hand_total.first(), Some(&4));
    for seat in 0..2 {
        assert_eq!(engine.state.players[seat].hand.len(), 1);
        assert_eq!(engine.state.players[seat].graveyard.len(), 1);
    }
}
