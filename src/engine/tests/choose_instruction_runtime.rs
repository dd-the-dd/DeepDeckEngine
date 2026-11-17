//! Runtime proof of the shared `choose` instruction frame: a creature type
//! chosen while the effect resolves (CR 608.2d) is read by a later battlefield
//! count of the same program, through the parsed Oracle IR.
use super::*;

/// Picks the resolution option whose single value is the requested word.
struct ChooseOption(&'static str);

impl DecisionProvider for ChooseOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        request
            .options
            .iter()
            .position(|action| {
                action.decisions.values().any(|value| {
                    value.as_str() == Some(self.0)
                        || value.as_array().is_some_and(|values| {
                            values.first().and_then(Value::as_str) == Some(self.0)
                        })
                })
            })
            .ok_or_else(|| EngineError::new("the requested resolution option is offered"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line, "power": "1", "toughness": "1",
    }))
    .expect("standalone test definition is valid")
}

fn permanent(id: &str, type_line: &str, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition(id, type_line), 0);
    instance.instance_id = id.into();
    instance
}

fn engine_with_tribes() -> GameEngine {
    let filler = definition("filler", "Basic Land — Plains");
    let mut engine = GameEngine::new(
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
        608,
    )
    .expect("standalone test engine is valid");
    for (id, type_line) in [
        ("elf-a", "Creature — Elf Druid"),
        ("elf-b", "Creature — Elf Warrior"),
        ("goblin", "Creature — Goblin"),
        ("elf-totem", "Artifact — Elf"),
    ] {
        engine.state.players[0]
            .battlefield
            .push(permanent(id, type_line, "player-0"));
    }
    // Another player's Elf is outside `you control`.
    engine.state.players[1]
        .battlefield
        .push(permanent("rival-elf", "Creature — Elf", "player-1"));
    engine
}

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{3}{U}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("choose-instruction-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status == "canonical" && ability.rule.is_some())
        .unwrap_or_else(|| panic!("a canonical rule: {parsed:#?}"));
    let rule = ability.rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn resolve(engine: &mut GameEngine, rule: &Value, chosen: &'static str) {
    let stack_object = StackObject {
        id: "stack:choose".to_string(),
        controller: "player-0".to_string(),
        card: permanent("choose-source", "Sorcery", "player-0"),
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
            &mut ChooseOption(chosen),
        )
        .expect("the parsed choose program resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn distant_melody_counts_permanents_of_the_type_chosen_on_resolution() {
    let rule = parsed_rule(
        "Distant Melody",
        "Sorcery",
        "Choose a creature type. Draw a card for each permanent you control of that type.",
    );
    assert_eq!(rule["effects"][0]["kind"], "chooseCreatureType");
    for (chosen, drawn) in [("Elf", 3), ("Goblin", 1), ("Merfolk", 0)] {
        let mut engine = engine_with_tribes();
        resolve(&mut engine, &rule, chosen);
        // Three Elf permanents (one an artifact) are counted; the rival's is not.
        assert_eq!(engine.state.players[0].hand.len(), drawn, "{chosen}");
        let chosen_event = engine
            .state
            .events
            .iter()
            .find(|event| event.kind == "creatureTypeChosen")
            .expect("the resolution choice is recorded");
        assert_eq!(chosen_event.detail["creatureType"], chosen);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn chosen_type_composes_with_a_creature_count_and_another_action() {
    // New combination: a creature-only count scales a life gain.
    let rule = parsed_rule(
        "Generalized Tribal Rain",
        "Instant",
        "Choose a creature type. You gain 2 life for each creature you control of the chosen type.",
    );
    let mut engine = engine_with_tribes();
    resolve(&mut engine, &rule, "Elf");
    // The Elf artifact is not a creature.
    assert_eq!(engine.state.players[0].life, 24);
}
