//! Runtime proofs of the choose-and-target slot widening, through parsed
//! Oracle IR:
//! * a postposed `where X is` amount read by a target declaration is read
//!   each time candidates are enumerated (CR 601.2c, 603.3d, 608.2b);
//! * one land of each basic land type is one choice per type whose union
//!   `those lands` reads (CR 305.6).
use super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("a decision with at least one option"))
    }
}

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line, "manaCost": mana_cost,
        "power": "1", "toughness": "1",
    }))
    .expect("standalone test definition is valid")
}

fn instance(id: &str, type_line: &str, mana_cost: &str, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition(id, type_line, mana_cost), 0);
    instance.instance_id = id.into();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land — Plains", "");
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
        603,
    )
    .expect("standalone test engine is valid")
}

fn parsed_rule(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("where-x-declaration-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status == "canonical" && ability.rule.is_some())
        .unwrap_or_else(|| panic!("a canonical rule: {parsed:#?}"));
    let rule = ability.rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn spell_on_stack(engine: &mut GameEngine, id: &str, mana_cost: &str) {
    engine.state.stack.push(StackObject {
        id: id.to_string(),
        controller: "player-1".to_string(),
        card: instance(id, "Sorcery", mana_cost, "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
}

fn stack_targets(candidates: &[TargetRef]) -> Vec<&str> {
    candidates
        .iter()
        .filter_map(|target| match target {
            TargetRef::StackObject { stack_id } => Some(stack_id.as_str()),
            _ => None,
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn spellstutter_sprite_cannot_target_a_spell_above_the_faerie_count() {
    let rule = parsed_rule(
        "Spellstutter Sprite",
        "Creature — Faerie Wizard",
        "{1}{U}",
        "Flash\nFlying\nWhen this creature enters, counter target spell with mana value X or less, where X is the number of Faeries you control.",
    );
    let decision = rule["declaration"]["decisions"]
        .as_array()
        .and_then(|decisions| {
            decisions
                .iter()
                .find(|decision| decision["kind"] == "chooseTargets")
        })
        .cloned()
        .expect("the declared spell target");
    assert!(target_candidates_supported(&decision["candidates"]));
    let mut engine = engine();
    for id in ["sprite", "faerie-b"] {
        engine.state.players[0].battlefield.push(instance(
            id,
            "Creature — Faerie",
            "{U}",
            "player-0",
        ));
    }
    // A Faerie of the opponent is outside `you control`.
    engine.state.players[1].battlefield.push(instance(
        "rival-faerie",
        "Creature — Faerie",
        "{U}",
        "player-1",
    ));
    spell_on_stack(&mut engine, "bolt", "{R}");
    spell_on_stack(&mut engine, "divination", "{2}{U}");
    let candidates = engine
        .target_candidates("player-0", &decision["candidates"])
        .expect("candidates are enumerated");
    assert_eq!(stack_targets(&candidates), ["bolt"]);
    // A third Faerie raises the threshold when the candidates are read again
    // (CR 608.2b rechecks against the current count).
    engine.state.players[0].battlefield.push(instance(
        "faerie-c",
        "Creature — Faerie",
        "{U}",
        "player-0",
    ));
    let candidates = engine
        .target_candidates("player-0", &decision["candidates"])
        .expect("candidates are enumerated");
    assert_eq!(stack_targets(&candidates), ["bolt", "divination"]);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn one_land_of_each_basic_land_type_is_destroyed() {
    let rule = parsed_rule(
        "Basic Land Census",
        "Sorcery",
        "{4}",
        "Choose a land of each basic land type, then destroy those lands.",
    );
    let mut engine = engine();
    for (id, type_line) in [
        ("plains", "Basic Land — Plains"),
        ("island", "Basic Land — Island"),
        ("swamp", "Basic Land — Swamp"),
        ("mountain", "Basic Land — Mountain"),
        ("forest-a", "Basic Land — Forest"),
        ("forest-b", "Basic Land — Forest"),
        ("wastes", "Basic Land"),
    ] {
        engine.state.players[1]
            .battlefield
            .push(instance(id, type_line, "", "player-1"));
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture lands are admitted");
    let stack_object = StackObject {
        id: "stack:census".to_string(),
        controller: "player-0".to_string(),
        card: instance("census", "Sorcery", "{4}", "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the parsed partitioned choice resolves");
    let remaining = engine.state.players[1]
        .battlefield
        .iter()
        .map(|permanent| permanent.instance_id.as_str())
        .collect::<Vec<_>>();
    // One land per basic land type is destroyed: one Forest and the land
    // without a basic land type remain.
    assert_eq!(remaining.len(), 2, "{remaining:?}");
    assert!(remaining.contains(&"wastes"), "{remaining:?}");
    assert!(
        remaining.contains(&"forest-a") != remaining.contains(&"forest-b"),
        "{remaining:?}"
    );
}
