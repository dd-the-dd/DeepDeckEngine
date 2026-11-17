//! Runtime proof of compiled `equal to` damage quantities (subject power,
//! sacrificed cost object read as last known information) and of the
//! mandatory `ACTION. When you do, BODY.` reflexive program (CR 603.12).
//! Every rule below is the production compiler's output for Oracle text.
use super::*;

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

fn definition(id: &str, type_line: &str, power: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some(power.to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", "0"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2024,
    )
    .unwrap()
}

fn rules(name: &str, type_line: &str, oracle_text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed
        .abilities
        .into_iter()
        .map(|ability| {
            let rule = ability.rule.expect("canonical rule");
            assert!(rule_is_executable(&rule), "{rule:#}");
            rule
        })
        .collect()
}

fn stack_object(card: CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:probe".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: rule["kind"].as_str().map(str::to_string),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn target_player(stack: &mut StackObject, rule: &Value, player: &str) {
    let id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target");
    stack.targets.insert(
        id.to_string(),
        TargetRef::Player {
            player_id: player.to_string(),
        },
    );
}

fn run(engine: &mut GameEngine, stack: &StackObject, decisions: BTreeMap<String, Value>) {
    let effects = stack.ability_rule.as_ref().unwrap()["effects"].clone();
    let mut decisions = decisions;
    execute_effects_at_resolution_start(
        engine,
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut decisions,
        &mut FirstOption,
    )
    .expect("the compiled program executes");
}

#[test]
fn juri_deals_damage_equal_to_its_last_known_power() {
    let rule = rules(
        "Juri, Master of the Revue",
        "Legendary Creature — Human Shaman",
        "When Juri dies, it deals damage equal to its power to any target.",
    )
    .remove(0);
    let mut engine = engine();
    // The source already left the battlefield: the stack copy is its LKI.
    let source = card_instance("player-0", &definition("juri", "Creature", "3"), 900);
    let mut stack = stack_object(source, &rule);
    target_player(&mut stack, &rule, "player-1");
    run(&mut engine, &stack, BTreeMap::new());
    assert_eq!(engine.state.players[1].life, 17);
}

#[test]
fn fling_reads_the_sacrificed_creatures_power_when_the_cost_was_paid() {
    let fling = rules(
        "Fling",
        "Instant",
        "As an additional cost to cast this spell, sacrifice a creature.\nFling deals damage equal to the sacrificed creature's power to any target.",
    );
    let mut engine = engine();
    let mut spell = definition("fling", "Instant", "0");
    spell.rules = fling.clone();
    let mut ogre = card_instance("player-0", &definition("ogre", "Creature", "4"), 901);
    ogre.counters.insert("+1/+1".to_string(), 1);
    let ogre_id = ogre.instance_id.clone();
    engine.state.players[0].battlefield.push(ogre);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let cost_id = fling[0]["declaration"]["additionalCosts"][0]["permanent"]["id"]
        .as_str()
        .expect("additional sacrifice cost id")
        .to_string();
    let targets = BTreeMap::from([(
        cost_id,
        TargetRef::Permanent {
            instance_id: ogre_id,
        },
    )]);
    // Read before the permanent leaves: current power including counters.
    let bound =
        engine.casting_sacrifice_cost_bindings(&spell, "player-0", &BTreeMap::new(), &targets);
    assert_eq!(bound, vec![("sacrificedPermanentPower".to_string(), 5)]);

    let card = card_instance("player-0", &spell, 902);
    let mut stack = stack_object(card, &fling[1]);
    target_player(&mut stack, &fling[1], "player-1");
    let decisions = bound
        .into_iter()
        .map(|(id, value)| (id, Value::from(value)))
        .collect();
    run(&mut engine, &stack, decisions);
    assert_eq!(engine.state.players[1].life, 15);
}

const REFLEXIVE: &str = "When this creature enters, sacrifice it. When you do, you gain 3 life.";

#[test]
fn a_performed_mandatory_action_creates_its_reflexive_trigger() {
    let rule = rules("Reflexive Probe", "Creature — Wall", REFLEXIVE).remove(0);
    assert_eq!(rule["effects"][0]["kind"], "performAction");
    let mut engine = engine();
    let source = card_instance("player-0", &definition("wall", "Creature", "0"), 903);
    engine.state.players[0].battlefield.push(source.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let stack = stack_object(source, &rule);
    let before = engine.state.stack.len();
    run(&mut engine, &stack, BTreeMap::new());
    assert!(engine.state.players[0].battlefield.is_empty());
    assert_eq!(engine.state.stack.len(), before + 1);
    let reflexive = engine.state.stack.last().unwrap();
    assert_eq!(
        reflexive.ability_rule.as_ref().unwrap()["effects"][0]["kind"],
        "gainLife"
    );
}

#[test]
fn an_unperformed_mandatory_action_creates_no_reflexive_trigger() {
    let rule = rules("Reflexive Probe", "Creature — Wall", REFLEXIVE).remove(0);
    let mut engine = engine();
    // The source is no longer on the battlefield, so it can't be sacrificed.
    let source = card_instance("player-0", &definition("wall", "Creature", "0"), 904);
    let stack = stack_object(source, &rule);
    let before = engine.state.stack.len();
    run(&mut engine, &stack, BTreeMap::new());
    assert_eq!(engine.state.stack.len(), before);
    assert_eq!(engine.state.players[0].life, 20);
}
