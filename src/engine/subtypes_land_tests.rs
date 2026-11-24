//! Runtime proofs of land subtype sets (CR 205.3i, 305.6, 305.7): a land
//! that becomes an Island until end of turn has the Island type and taps for
//! {U} instead of its old color, loses the abilities of its rules text, and
//! recovers at cleanup; a land type added in addition keeps the old type and
//! mana; a non-land never carries a land type; unsupported land sets stay
//! explicit.
use super::super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("land subtype test decision has no option"))
    }
}

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: String::new(),
        name: id.to_string(),
        power: None,
        rules,
        toughness: None,
        type_line: type_line.to_string(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("land-subtype-filler", "Basic Land - Plains", Vec::new());
    GameEngine::new(
        GameSetup {
            opening_hand_size: 7,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 12],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        305,
    )
    .expect("land subtype test setup is valid")
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) -> String {
    let mut card = card_instance("player-0", &definition(id, type_line, rules), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    id.to_string()
}

fn permanent(engine: &GameEngine, id: &str) -> CardInstance {
    let (player, index) = engine.permanent_position(id).expect("permanent exists");
    engine.state.players[player].battlefield[index].clone()
}

fn is_subtype(engine: &GameEngine, id: &str, subtype: &str) -> bool {
    engine.permanent_matches_effective_filter(
        &permanent(engine, id),
        &json!({ "kind": "subtypeContains", "value": subtype }),
    )
}

fn mana(engine: &GameEngine, id: &str) -> Vec<String> {
    engine.mana_types_permanent_could_produce(&permanent(engine, id), &BTreeSet::new())
}

/// `Target land becomes LAND_TYPE` as the Oracle compiler emits it.
fn land_type_rule(mode: &str, subtypes: Value, duration: &str) -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseTargets",
                "id": "targetPermanent",
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Land" },
                },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "effects": [{
            "kind": "modifySubtypeSet",
            "mode": mode,
            "subtypeClass": "land",
            "subtypes": subtypes,
            "object": { "kind": "chosenTarget", "id": "targetPermanent" },
            "duration": { "kind": duration },
        }],
    })
}

fn resolve_on(engine: &mut GameEngine, source: &str, rule: &Value, target: &str) {
    let stack = StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: permanent(engine, source),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: target.to_string(),
            },
        )]),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the land subtype change resolves");
}

#[test]
fn tidal_warriors_target_becomes_an_island_that_taps_for_blue_until_end_of_turn() {
    let rule = land_type_rule("replace", json!(["Island"]), "untilEndOfCurrentTurn");
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let warrior = add(
        &mut engine,
        "tidal-warrior",
        "Creature - Merfolk Warrior",
        Vec::new(),
    );
    let forest = add(&mut engine, "forest", "Basic Land - Forest", Vec::new());
    assert_eq!(mana(&engine, &forest), ["G"]);

    resolve_on(&mut engine, &warrior, &rule, &forest);
    assert!(is_subtype(&engine, &forest, "Island"));
    assert!(
        !is_subtype(&engine, &forest, "Forest"),
        "old land types are lost"
    );
    assert_eq!(mana(&engine, &forest), ["U"], "{{U}}, never its old {{G}}");
    let land = permanent(&engine, &forest);
    assert_eq!(
        engine.static_basic_land_mana(&land),
        [vec!["U".to_string()]]
    );
    assert!(engine.mana_abilities_replaced_by_land_subtype(&land));

    // CR 514.2: the effect ends in the cleanup step.
    engine.cleanup(&mut FirstOption).expect("cleanup runs");
    assert!(is_subtype(&engine, &forest, "Forest"));
    assert!(!is_subtype(&engine, &forest, "Island"));
    assert_eq!(mana(&engine, &forest), ["G"]);
}

#[test]
fn a_land_whose_subtype_is_set_loses_the_abilities_of_its_rules_text() {
    let ability = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "drawCards",
            "count": { "kind": "integer", "value": 1 },
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        }],
    });
    // Thelonite Monk: `Target land becomes a Forest` indefinitely.
    let rule = land_type_rule("replace", json!(["Forest"]), "permanent");
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let monk = add(&mut engine, "monk", "Creature - Insect Monk", Vec::new());
    let library = add(&mut engine, "library", "Land", vec![ability.clone()]);
    let has_ability = |engine: &GameEngine| {
        let land = permanent(engine, "library");
        engine
            .effective_rules(&land)
            .iter()
            .any(|rule| *rule == ability)
    };
    assert!(has_ability(&engine));
    resolve_on(&mut engine, &monk, &rule, &library);
    assert!(
        !has_ability(&engine),
        "CR 305.7: rules-text abilities are lost"
    );
    assert!(is_subtype(&engine, &library, "Forest"));
    assert_eq!(mana(&engine, &library), ["G"]);
}

#[test]
fn a_land_type_added_in_addition_keeps_the_old_type_and_mana() {
    // The Flood of Mars: `it becomes an Island in addition to its other types`.
    let rule = land_type_rule("add", json!(["Island"]), "permanent");
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let source = add(&mut engine, "flood", "Creature - Elemental", Vec::new());
    let mountain = add(&mut engine, "mountain", "Basic Land - Mountain", Vec::new());
    resolve_on(&mut engine, &source, &rule, &mountain);
    assert!(is_subtype(&engine, &mountain, "Island"));
    assert!(is_subtype(&engine, &mountain, "Mountain"));
    assert_eq!(mana(&engine, &mountain), ["U", "R"]);
}

#[test]
fn a_nonland_never_gains_a_land_type() {
    let rule = land_type_rule("replace", json!(["Island"]), "untilEndOfCurrentTurn");
    let mut engine = engine();
    let source = add(&mut engine, "source", "Creature - Merfolk", Vec::new());
    let bear = add(&mut engine, "bear", "Creature - Bear", Vec::new());
    resolve_on(&mut engine, &source, &rule, &bear);
    assert!(!is_subtype(&engine, &bear, "Island"));
    assert!(is_subtype(&engine, &bear, "Bear"));
}

#[test]
fn unsupported_land_subtype_sets_stay_explicit() {
    for (mode, subtypes) in [
        // A creature type is not a land type.
        ("replace", json!(["Elf"])),
        // Setting a nonbasic land type is not CR 305.7.
        ("replace", json!(["Desert"])),
        ("all", json!(["Island"])),
        ("add", json!([])),
    ] {
        let rule = land_type_rule(mode, subtypes.clone(), "untilEndOfCurrentTurn");
        assert!(!rule_is_executable(&rule), "{mode} {subtypes}");
    }
    assert!(rule_is_executable(&land_type_rule(
        "add",
        json!(["Desert"]),
        "permanent"
    )));
}
