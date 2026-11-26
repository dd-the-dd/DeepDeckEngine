//! Runtime proofs of program control over bound sets (CR 608.2c): `For each
//! PLAYER, ...`, `For each of SET, ...` and `repeat this process`. Each card
//! test plays the real Oracle text through the compiler and the engine.
use super::*;

fn compile(name: &str, type_line: &str, text: &str) -> CardDefinition {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let is_creature = type_line.contains("Creature");
    let compiled = std::thread::Builder::new()
        .name("program-iteration-compiler".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            compile_playable_card(PlayableCardInput {
                id: "iteration-source".to_string(),
                face_id: None,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                power: is_creature.then(|| "2".to_string()),
                toughness: is_creature.then(|| "2".to_string()),
                oracle: request,
            })
        })
        .expect("compiler thread starts")
        .join()
        .expect("compiler thread completes")
        .expect("the card compiles");
    assert_eq!(compiled.engine_status, "executable", "{name}");
    compiled.card
}

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.rule.is_some())
        .unwrap_or_else(|| panic!("{name}: {:#?}", parsed.abilities));
    assert_eq!(
        ability.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    let rule = ability.rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Giant");
    definition.power = Some("4".to_string());
    definition.toughness = Some("5".to_string());
    test_instance(id, definition, controller)
}

fn permanent(id: &str, type_line: &str, controller: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), controller)
}

/// Performs every offered optional action; any other decision takes its
/// first option.
struct Perform;

impl DecisionProvider for Perform {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| {
                option.kind == ActionKind::ChooseResolution
                    && option
                        .decisions
                        .get("performOptionalAction")
                        .is_some_and(|value| value == &Value::Bool(true))
            })
            .unwrap_or(0))
    }
}

/// Casts the compiled sorcery for free declaring exactly the permanents
/// `wanted` through the legal cast actions, and leaves it on the stack.
fn cast(engine: &mut GameEngine, card: &CardDefinition, wanted: &[&str]) {
    engine.state.players[0]
        .hand
        .push(test_instance("iteration-source", card.clone(), "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let wanted = wanted
        .iter()
        .map(|id| id.to_string())
        .collect::<BTreeSet<_>>();
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| action.kind == ActionKind::CastSpell && declared(action) == wanted)
        .unwrap_or_else(|| panic!("no cast declaring exactly {wanted:?}"));
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell is cast: {}", error.message));
}

fn declared(action: &LegalAction) -> BTreeSet<String> {
    action
        .targets
        .values()
        .filter_map(|target| match target {
            TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
            _ => None,
        })
        .collect()
}

fn resolve(engine: &mut GameEngine) {
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell resolves: {}", error.message));
}

fn tokens_named(engine: &GameEngine, player: usize, name: &str) -> usize {
    engine.state.players[player]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.is_token && permanent.definition.name == name)
        .count()
}

fn on_battlefield(engine: &GameEngine, id: &str) -> bool {
    engine.permanent_position(id).is_some()
}

fn stack_object(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:program-iteration".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn gain_one_life() -> Value {
    json!({
        "kind": "gainLife",
        "player": {"kind": "controllerOf", "object": {"kind": "self"}},
        "amount": {"kind": "integer", "value": 1},
    })
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_repeated_process_is_performed_again_up_to_its_bound() {
    let mut engine = test_engine(2);
    let source = permanent("source", "Sorcery", "player-0");
    let process = json!({
        "kind": "repeatableProcess",
        "process": "repeatedProcess",
        "maxRepeats": {"kind": "integer", "value": 2},
        "effects": [gain_one_life(), {"kind": "repeatProcess", "process": "repeatedProcess"}],
    });
    assert!(effect_supported(&process), "{process:#}");
    let rule = json!({"effects": [process.clone()]});
    let life = engine.state.players[0].life;
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object(&source, &rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Perform,
        )
        .expect("the bounded process resolves");
    // The first performance and two more (`two more times`).
    assert_eq!(engine.state.players[0].life, life + 3);
    // A pass that does not reach its repetition ends the process.
    let once = json!({
        "kind": "repeatableProcess",
        "process": "repeatedProcess",
        "effects": [gain_one_life(), {
            "kind": "conditional",
            "condition": {"kind": "compare", "operator": ">",
                "left": {"kind": "integer", "value": 0},
                "right": {"kind": "integer", "value": 1}},
            "then": [{"kind": "repeatProcess", "process": "repeatedProcess"}],
            "else": [],
        }],
    });
    let rule = json!({"effects": [once]});
    let life = engine.state.players[0].life;
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object(&source, &rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Perform,
        )
        .expect("the process resolves once");
    assert_eq!(engine.state.players[0].life, life + 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_unbounded_repetition_past_the_resource_limit_is_refused() {
    let mut engine = test_engine(2);
    let source = permanent("source", "Sorcery", "player-0");
    let process = json!({
        "kind": "repeatableProcess",
        "process": "repeatedProcess",
        "effects": [gain_one_life(), {"kind": "repeatProcess", "process": "repeatedProcess"}],
    });
    let rule = json!({"effects": [process.clone()]});
    let error = engine
        .execute_repeatable_process(
            &process,
            &stack_object(&source, &rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Perform,
            4,
        )
        .expect_err("an endless repetition is refused, never silently stopped");
    assert!(error.message.contains("refuses"), "{}", error.message);
    // A repetition the drafter could not resolve is never executable.
    let unresolved = json!({"kind": "unresolvedRepeatedProcess", "process": "repeatedProcess"});
    assert!(!effect_supported(&unresolved));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn cultivator_colossus_repeats_while_a_land_card_is_put_onto_the_battlefield() {
    let rule = parsed_rule(
        "Cultivator Colossus",
        "Creature \u{2014} Plant Beast",
        "When this creature enters, you may put a land card from your hand onto the battlefield tapped. If you do, draw a card and repeat this process.",
    );
    assert_eq!(rule["effects"][0]["kind"], "repeatableProcess", "{rule:#}");
    let mut engine = test_engine(2);
    let source = creature("colossus", "player-0");
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[0].hand = vec![
        permanent("forest-a", "Basic Land \u{2014} Forest", "player-0"),
        permanent("forest-b", "Basic Land \u{2014} Forest", "player-0"),
        permanent("shock", "Instant", "player-0"),
    ];
    engine.state.players[0].library = (0..4)
        .map(|index| permanent(&format!("spell-{index}"), "Sorcery", "player-0"))
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object(&source, &rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Perform,
        )
        .expect("the trigger resolves");
    // Two lands put, two cards drawn; the third pass has no land to put,
    // so it is not performed and the process ends (bounded by the supply).
    assert!(on_battlefield(&engine, "forest-a"));
    assert!(on_battlefield(&engine, "forest-b"));
    assert_eq!(engine.state.players[0].library.len(), 2);
    assert_eq!(engine.state.players[0].hand.len(), 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_body_without_a_member_reference_is_performed_once_per_opponent() {
    let card = compile(
        "Endless Ranks of HYDRA",
        "Sorcery",
        "For each opponent, you create a 2/1 black Villain creature token with menace.",
    );
    let mut engine = test_engine(3);
    engine.state.step = GameStep::PrecombatMain;
    cast(&mut engine, &card, &[]);
    resolve(&mut engine);
    assert_eq!(tokens_named(&engine, 0, "Villain Token"), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_per_player_target_admits_one_permanent_of_each_opponent() {
    let card = compile(
        "Dismantling Wave",
        "Sorcery",
        "For each opponent, destroy up to one target artifact or enchantment that player controls.",
    );
    let mut engine = test_engine(3);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![permanent("own-relic", "Artifact", "player-0")];
    engine.state.players[1].battlefield = vec![
        permanent("first-relic-a", "Artifact", "player-1"),
        permanent("first-relic-b", "Artifact", "player-1"),
    ];
    engine.state.players[2].battlefield = vec![permanent("second-relic", "Artifact", "player-2")];
    engine.state.players[0]
        .hand
        .push(test_instance("iteration-source", card.clone(), "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let casts = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| action.kind == ActionKind::CastSpell)
        .collect::<Vec<_>>();
    // CR 601.2c: one permanent of each opponent, never two of the same one,
    // never the controller's own.
    assert!(
        casts
            .iter()
            .all(|action| !declared(action).contains("own-relic"))
    );
    assert!(casts.iter().all(|action| {
        let targets = declared(action);
        !(targets.contains("first-relic-a") && targets.contains("first-relic-b"))
    }));
    let wanted = ["first-relic-a", "second-relic"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let cast = casts
        .into_iter()
        .find(|action| declared(action) == wanted)
        .expect("one artifact of each opponent is a legal declaration");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell is cast: {}", error.message));
    resolve(&mut engine);
    assert!(!on_battlefield(&engine, "first-relic-a"));
    assert!(!on_battlefield(&engine, "second-relic"));
    assert!(on_battlefield(&engine, "first-relic-b"));
    assert!(on_battlefield(&engine, "own-relic"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn each_declared_member_gets_its_own_copy_and_a_departed_member_none() {
    let card = compile(
        "Hate Mirage",
        "Sorcery",
        "Choose up to two target creatures you don't control. For each of those creatures, create a token that's a copy of that creature. Those tokens gain haste. Exile them at the beginning of the next end step.",
    );
    // Both members are copied.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[1].battlefield = vec![
        creature("opp-giant-a", "player-1"),
        creature("opp-giant-b", "player-1"),
    ];
    cast(&mut engine, &card, &["opp-giant-a", "opp-giant-b"]);
    resolve(&mut engine);
    assert_eq!(tokens_named(&engine, 0, "opp-giant-a"), 1);
    assert_eq!(tokens_named(&engine, 0, "opp-giant-b"), 1);
    // A member that left before resolution is no longer a legal target
    // (CR 608.2b): the body is performed for the remaining member only.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[1].battlefield = vec![
        creature("opp-giant-a", "player-1"),
        creature("opp-giant-b", "player-1"),
    ];
    cast(&mut engine, &card, &["opp-giant-a", "opp-giant-b"]);
    let departed = engine.state.players[1]
        .battlefield
        .iter()
        .position(|permanent| permanent.instance_id == "opp-giant-b")
        .expect("the member is on the battlefield");
    let departed = engine.state.players[1].battlefield.remove(departed);
    engine.state.players[1].graveyard.push(departed);
    resolve(&mut engine);
    assert_eq!(tokens_named(&engine, 0, "opp-giant-a"), 1);
    assert_eq!(tokens_named(&engine, 0, "opp-giant-b"), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_member_relation_reads_each_member_in_turn() {
    // A combination of supported concepts not used to design the frame:
    // the member's controller as the performer of a life action.
    let card = compile(
        "Member Relation Probe",
        "Sorcery",
        "Choose up to two target creatures. For each of those creatures, its controller gains 2 life.",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![creature("own-giant", "player-0")];
    engine.state.players[1].battlefield = vec![creature("opp-giant", "player-1")];
    let (own, opponent) = (engine.state.players[0].life, engine.state.players[1].life);
    cast(&mut engine, &card, &["own-giant", "opp-giant"]);
    resolve(&mut engine);
    assert_eq!(engine.state.players[0].life, own + 2);
    assert_eq!(engine.state.players[1].life, opponent + 2);
}
