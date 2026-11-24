//! Runtime proofs of the characteristic-change domain: layer-7a
//! characteristic-defining P/T in and out of the battlefield, a chosen color
//! set, and a token copy with copiable exceptions and an end-step sacrifice.
use super::super::super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("characteristic test decision has no option"))
    }
}

fn definition(id: &str, type_line: &str, power: &str, toughness: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{2}{G}".to_string(),
        name: id.to_string(),
        power: Some(power.to_string()),
        rules: Vec::new(),
        toughness: Some(toughness.to_string()),
        type_line: type_line.to_string(),
    }
}

fn instance(instance_id: &str, definition: CardDefinition) -> CardInstance {
    let mut instance = card_instance("player-0", &definition, 0);
    instance.instance_id = instance_id.to_string();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("characteristic-filler", "Basic Land - Forest", "0", "0");
    let mut filler = filler;
    filler.power = None;
    filler.toughness = None;
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
    .expect("characteristic test setup is valid")
}

/// A layer-7a CDA as the Oracle lowering emits it.
fn cda_rule(modifier: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "characteristicDefining": true,
        "modifiers": [modifier],
    })
}

fn stack_for(card: CardInstance, rule: Value) -> StackObject {
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn characteristic_defining_power_and_toughness_apply_in_layer_7a_in_every_zone() {
    let mut engine = engine();
    let mut goyf = definition("goyf", "Creature - Lhurgoyf", "*", "1+*");
    goyf.rules = vec![cda_rule(json!({
        "kind": "setBasePowerToughness",
        "objects": { "kind": "self" },
        "layer": "7a",
        "power": { "kind": "integer", "value": 3 },
        "toughness": { "kind": "integer", "value": 4 },
    }))];
    assert!(rule_is_executable(&goyf.rules[0]));
    engine.state.players[0]
        .graveyard
        .push(instance("goyf-card", goyf.clone()));
    engine.state.players[0]
        .battlefield
        .push(instance("goyf", goyf));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    // On the battlefield: the CDA defines the base, never a layer-7b effect.
    let permanent = engine.state.players[0].battlefield.last().cloned().unwrap();
    assert_eq!(engine.current_power(&permanent), 3);
    assert_eq!(engine.current_toughness(&permanent), 4);

    // Outside the battlefield: the same CDA functions (CR 604.3); an object
    // condition reads it instead of refusing the non-numeric printed `*`.
    let card = engine.state.players[0].graveyard.last().cloned().unwrap();
    assert_eq!(
        engine.characteristic_defining_power_toughness(&card),
        Some((3, 4))
    );
    let stack = stack_for(card, json!({ "kind": "activatedAbility" }));
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": { "kind": "self" },
        "where": {
            "kind": "compare",
            "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
            "operator": ">=",
            "right": { "kind": "integer", "value": 3 },
        },
    });
    assert_eq!(
        engine.checked_object_condition_matches(
            &condition,
            &stack,
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        Ok(true)
    );

    // A resolved layer-7b setting effect overrides the 7a value.
    let source = engine.state.players[0].battlefield.last().cloned().unwrap();
    let stack = stack_for(source, json!({ "kind": "activatedAbility" }));
    engine
        .execute_effects(
            &json!([{
                "kind": "setBasePowerToughness",
                "object": { "kind": "self" },
                "power": { "kind": "integer", "value": 1 },
                "toughness": { "kind": "integer", "value": 1 },
                "duration": { "kind": "untilEndOfCurrentTurn" },
            }]),
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("layer-7b base power/toughness resolves");
    let permanent = engine.state.players[0].battlefield.last().cloned().unwrap();
    assert_eq!(engine.current_power(&permanent), 1);
    assert_eq!(engine.current_toughness(&permanent), 1);
}

#[test]
fn a_power_only_cda_keeps_the_printed_toughness_and_ability_loss_removes_it() {
    let mut engine = engine();
    let mut drake = definition("drake", "Creature - Drake", "*", "4");
    drake.rules = vec![cda_rule(json!({
        "kind": "setPower",
        "objects": { "kind": "self" },
        "layer": "7a",
        "power": { "kind": "integer", "value": 5 },
    }))];
    engine.state.players[0]
        .battlefield
        .push(instance("drake", drake));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let permanent = engine.state.players[0].battlefield.last().cloned().unwrap();
    assert_eq!(engine.current_power(&permanent), 5);
    assert_eq!(engine.current_toughness(&permanent), 4);

    // Layer 6 removes the CDA before layer 7a applies: `*` is then 0.
    engine.state.rule_modifiers.push(json!({
        "kind": "loseAllAbilities",
        "cardInstanceId": "drake",
        "expiresAfterTurn": engine.state.turn_number,
    }));
    let permanent = engine.state.players[0].battlefield.last().cloned().unwrap();
    assert_eq!(
        engine.characteristic_defining_power_toughness(&permanent),
        None
    );
    assert_eq!(engine.current_power(&permanent), 0);
}

#[test]
fn a_chosen_color_set_reads_its_decision_and_a_missing_choice_is_an_error() {
    let mut engine = engine();
    engine.state.players[0].battlefield.push(instance(
        "bear",
        definition("bear", "Creature - Bear", "2", "2"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = engine.state.players[0].battlefield.last().cloned().unwrap();
    let stack = stack_for(source, json!({ "kind": "activatedAbility" }));
    let effect = json!([{
        "kind": "setColors",
        "object": { "kind": "self" },
        "colors": { "kind": "decisionResult", "decisionId": "chosenColors" },
        "duration": { "kind": "untilEndOfCurrentTurn" },
    }]);
    assert!(
        engine
            .execute_effects(
                &effect,
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut FirstOption,
            )
            .is_err(),
        "a missing color choice never becomes colorless"
    );
    let mut decisions = BTreeMap::from([("chosenColors".to_string(), json!(["U", "R"]))]);
    engine
        .execute_effects(
            &effect,
            &stack,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut FirstOption,
        )
        .expect("the chosen colors are set");
    let permanent = engine.state.players[0].battlefield.last().unwrap();
    assert_eq!(
        super::marker_colors(&permanent.definition),
        Some(vec!["U".to_string(), "R".to_string()])
    );
}

#[test]
fn a_token_copy_applies_copiable_exceptions_binds_the_token_and_is_sacrificed_later() {
    let mut engine = engine();
    let mut copied = definition("legend", "Legendary Creature - Human Wizard", "3", "3");
    copied.rules = vec![
        json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": { "kind": "flying" } }),
        // A resolved characteristic change is not a copiable value.
        json!({ "kind": super::COLOR_SET_MARKER, "colors": ["B"] }),
    ];
    engine.state.players[0]
        .battlefield
        .push(instance("legend", copied));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let legend_id = engine.state.players[0]
        .battlefield
        .last()
        .unwrap()
        .instance_id
        .clone();
    let effect = json!({
        "kind": "createTokens",
        "quantity": { "kind": "integer", "value": 1 },
        "token": {
            "kind": "copyOf",
            "object": { "kind": "boundObjects", "binding": "copied" },
            "except": [
                { "kind": "addSubtypes", "subtypes": ["Nightmare"] },
                { "kind": "grantKeyword", "keyword": "haste" },
                { "kind": "removeSupertype", "supertype": "Legendary" },
                {
                    "kind": "setBasePowerToughness",
                    "power": { "kind": "integer", "value": 1 },
                    "toughness": { "kind": "integer", "value": 1 },
                },
            ],
        },
        "sacrificeAtNextEndStep": true,
        "bind": "createdTokens",
    });
    let rule = json!({ "kind": "spellAbility", "source": { "kind": "self" }, "effects": [effect] });
    assert!(rule_is_executable(&rule), "{rule}");
    let source = engine.state.players[0].battlefield.last().cloned().unwrap();
    let stack = stack_for(source, rule.clone());
    let mut bindings = BTreeMap::from([(
        "copied".to_string(),
        RuntimeBinding::Objects(vec![legend_id.clone()]),
    )]);
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the token copy resolves");

    let token = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.is_token)
        .cloned()
        .expect("one token copy");
    assert_eq!(token.definition.name, "legend");
    assert_eq!(token.definition.power.as_deref(), Some("1"));
    assert_eq!(token.definition.toughness.as_deref(), Some("1"));
    assert!(!token.definition.type_line.contains("Legendary"));
    assert!(token.definition.type_line.contains("Nightmare"));
    assert!(engine.permanent_has_effective_keyword(&token, "flying"));
    assert!(engine.permanent_has_effective_keyword(&token, "haste"));
    assert!(
        token
            .definition
            .printed_rules()
            .iter()
            .all(|rule| value_kind(rule) != Some(super::COLOR_SET_MARKER))
    );
    assert!(matches!(
        bindings.get("createdTokens"),
        Some(RuntimeBinding::Objects(ids)) if ids == &vec![token.instance_id.clone()]
    ));
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("sacrificeAtEndStep")
            && modifier["cardInstanceId"].as_str() == Some(token.instance_id.as_str())
    }));
    // The copied permanent is unchanged.
    let (player, index) = engine.permanent_position(&legend_id).unwrap();
    assert!(
        engine.state.players[player].battlefield[index]
            .definition
            .type_line
            .starts_with("Legendary")
    );
}
