use super::super::*;

struct SubtypeDecisionProvider;

impl DecisionProvider for SubtypeDecisionProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("subtype test decision has no option"))
    }
}

fn subtype_test_definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: id.to_string(),
        power: None,
        rules: Vec::new(),
        toughness: None,
        type_line: type_line.to_string(),
    }
}

fn subtype_test_instance(
    instance_id: &str,
    definition: CardDefinition,
    owner: &str,
) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = instance_id.to_string();
    instance
}

fn subtype_test_engine() -> GameEngine {
    let filler = subtype_test_definition("subtype-test-filler", "Basic Land - Plains");
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
        120,
    )
    .expect("subtype test setup is valid")
}

fn subtype_rule() -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "payMana", "manaCost": "{4}" }],
        "effects": [
            {
                "kind": "becomeCreature",
                "object": { "kind": "self" },
                "addTypes": ["Creature"],
                "addSubtypes": [],
                "addColors": [],
                "basePower": { "kind": "integer", "value": 3 },
                "baseToughness": { "kind": "integer", "value": 3 },
                "retainExistingTypes": true,
                "duration": { "kind": "permanent" },
            },
            {
                "kind": "grantKeyword",
                "object": { "kind": "self" },
                "keyword": "vigilance",
                "duration": { "kind": "permanent" },
            },
            {
                "kind": "modifySubtypeSet",
                "object": { "kind": "self" },
                "subtypeClass": "creature",
                "mode": "all",
                "duration": { "kind": "permanent" },
            },
        ],
    })
}

fn setup() -> (GameEngine, Value, StackObject) {
    let rule = subtype_rule();
    assert!(rule_is_executable(&rule));
    let mut engine = subtype_test_engine();
    let source = subtype_test_instance(
        "animated-land",
        subtype_test_definition("animated-land", "Land"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(source);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    // The activated ability carries its source's admitted incarnation.
    let source = engine.state.players[0]
        .battlefield
        .last()
        .cloned()
        .expect("source is on the battlefield");
    let stack = StackObject {
        id: "stack:source-animation".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, rule, stack)
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack: &StackObject) {
    engine
        .execute_effects(
            &rule["effects"],
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut SubtypeDecisionProvider,
        )
        .expect("primitive source animation resolves");
}

#[test]
fn all_creature_subtypes_are_characteristics_independent_of_abilities() {
    let (mut engine, rule, stack) = setup();
    resolve(&mut engine, &rule, &stack);

    let permanent = &engine.state.players[0].battlefield[0];
    assert_eq!(permanent.definition.type_line, "Land Creature");
    assert_eq!(permanent.definition.power.as_deref(), Some("3"));
    assert_eq!(permanent.definition.toughness.as_deref(), Some("3"));
    assert!(engine.permanent_has_effective_keyword(permanent, "vigilance"));
    assert!(engine.permanent_matches_effective_filter(
        permanent,
        &json!({
            "kind": "subtypeContains",
            "subtypeClass": "creature",
            "value": "Elf",
        }),
    ));
    assert!(!engine.permanent_matches_effective_filter(
        permanent,
        &json!({
            "kind": "subtypeContains",
            "subtypeClass": "land",
            "value": "Plains",
        }),
    ));

    engine.state.rule_modifiers.push(json!({
        "kind": "loseAllAbilities",
        "cardInstanceId": "animated-land",
        "expiresAfterTurn": engine.state.turn_number,
    }));
    let permanent = &engine.state.players[0].battlefield[0];
    assert!(!engine.permanent_has_effective_keyword(permanent, "vigilance"));
    assert!(engine.permanent_matches_effective_filter(
        permanent,
        &json!({
            "kind": "subtypeContains",
            "subtypeClass": "creature",
            "value": "Brushwagg",
        }),
    ));
}

#[test]
fn subtype_set_modification_ends_when_the_object_changes_zones() {
    let (mut engine, rule, stack) = setup();
    resolve(&mut engine, &rule, &stack);
    engine
        .move_permanent_to_graveyard("animated-land", "destroyed")
        .expect("animated land leaves the battlefield");

    let card = engine.state.players[0]
        .graveyard
        .iter()
        .find(|card| card.instance_id == "animated-land")
        .expect("card is in its owner's graveyard");
    assert_eq!(card.definition.type_line, "Land");
    assert!(!super::definition_has_all_subtypes(
        &card.definition,
        "creature"
    ));
    assert!(!has_keyword(&card.definition, "vigilance"));
}

#[test]
fn subtype_set_contract_rejects_unknown_classes_modes_and_durations() {
    let valid = json!({
        "kind": "modifySubtypeSet",
        "object": { "kind": "self" },
        "subtypeClass": "creature",
        "mode": "all",
        "duration": { "kind": "permanent" },
    });
    assert!(effect_supported(&valid));
    for invalid in [
        json!({ "kind": "modifySubtypeSet", "object": { "kind": "self" }, "subtypeClass": "artifact", "mode": "all", "duration": { "kind": "permanent" } }),
        json!({ "kind": "modifySubtypeSet", "object": { "kind": "self" }, "subtypeClass": "creature", "mode": "replace", "duration": { "kind": "permanent" } }),
        json!({ "kind": "modifySubtypeSet", "object": { "kind": "self" }, "subtypeClass": "creature", "mode": "all", "duration": { "kind": "forever" } }),
    ] {
        assert!(!effect_supported(&invalid));
    }
}
