use super::*;

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn parsed_eldrazi_token_keeps_and_executes_its_embedded_mana_ability() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Spawn Maker".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{2}{C}".to_string()),
        oracle_text: Some(
            "Create a 0/1 colorless Eldrazi Spawn creature token. It has \"Sacrifice this token: Add {C}.\""
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("token creation parses canonically");
    let create_token = &rule["effects"][0];

    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:test-spawn-maker".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "test-spawn-maker",
            test_definition("test-spawn-maker", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .create_tokens(
            create_token,
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("create parsed Eldrazi Spawn token");

    let spawn = &engine.state.players[0].battlefield[0];
    assert_eq!(spawn.definition.name, "Eldrazi Spawn Token");
    assert_eq!(spawn.definition.rules[0]["kind"], "manaAbility");
    let spawn_id = spawn.instance_id.clone();
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some(spawn_id.as_str())
        })
        .expect("the parsed Spawn mana ability is legal");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(1))
        .expect("sacrifice Spawn for colorless mana");

    assert!(engine.state.players[0].battlefield.is_empty());
    assert_eq!(engine.state.players[0].mana_pool[0].symbol, "C");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn clue_token_uses_its_parsed_catalog_ability_to_draw_a_card() {
    let mut engine = test_engine(2);
    let clue_definition = compile_named_token_definition("Clue")
        .expect("token catalog loads")
        .expect("Clue definition compiles");
    engine.state.players[0].battlefield = vec![
        test_instance("clue", clue_definition, "player-0"),
        mana_source("wastes-1", "Basic Land - Wastes", "{C}", None),
        mana_source("wastes-2", "Basic Land - Wastes", "{C}", None),
    ];
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![test_instance(
        "clue-drawn-card",
        test_definition("clue-drawn-card", "Instant"),
        "player-0",
    )];

    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("clue")
        })
        .expect("Clue activation is legal");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(1))
        .expect("pay and sacrifice Clue");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|card| card.instance_id != "clue")
    );

    engine
        .resolve_top_stack(&mut RandomAi::seeded(1))
        .expect("resolve Clue draw");
    assert_eq!(
        engine.state.players[0].hand[0].instance_id,
        "clue-drawn-card"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn created_blood_token_uses_catalog_oracle_and_executes_all_of_its_costs() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield =
        vec![mana_source("wastes", "Basic Land - Wastes", "{C}", None)];
    engine.state.players[0].hand = vec![test_instance(
        "discarded-card",
        test_definition("discarded-card", "Sorcery"),
        "player-0",
    )];
    engine.state.players[0].library = vec![test_instance(
        "drawn-card",
        test_definition("drawn-card", "Instant"),
        "player-0",
    )];
    let mut source_definition = test_definition("blood-source", "Sorcery");
    source_definition.rules.push(json!({
        "kind": "rulesMarker",
        "text": "Source-linked token definitions",
        "relatedTokenOracle": [{
            "name": "blood",
            "typeLine": "Token Artifact - Blood Source Linked",
            "oracleText": "{1}, {T}, Discard a card, Sacrifice this token: Draw a card.",
            "manaCost": "",
        }],
    }));
    let stack_object = StackObject {
        id: "stack:create-blood".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("blood-source", source_definition, "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .create_tokens(
            &json!({
                "kind": "createTokens",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "quantity": { "kind": "integer", "value": 1 },
                "token": { "kind": "namedToken", "name": "Blood" },
            }),
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("create Blood token");

    let blood = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.name == "Blood")
        .expect("Blood is on the battlefield");
    let blood_id = blood.instance_id.clone();
    assert!(blood.definition.type_line.contains("Artifact"));
    assert!(blood.definition.type_line.contains("Source Linked"));
    assert_eq!(blood.definition.rules[0]["effects"][0]["kind"], "drawCards");
    assert!(
        blood.definition.rules[0]["costs"]
            .as_array()
            .is_some_and(|costs| { costs.iter().any(|cost| cost["kind"] == "discardCard") })
    );

    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some(blood_id.as_str())
        })
        .expect("Blood activation is legal");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(1))
        .expect("pay Blood activation costs");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|card| card.instance_id != blood_id)
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "discarded-card")
    );
    assert!(engine.state.players[0].hand.is_empty());

    engine
        .resolve_top_stack(&mut RandomAi::seeded(1))
        .expect("resolve Blood draw");
    assert_eq!(engine.state.players[0].hand[0].instance_id, "drawn-card");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn named_token_creation_honors_the_tapped_entry_flag() {
    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:create-tapped-treasure".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "treasure-source",
            test_definition("treasure-source", "Creature"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .create_tokens(
            &json!({
                "kind": "createTokens",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "quantity": { "kind": "integer", "value": 1 },
                "token": { "kind": "namedToken", "name": "Treasure" },
                "tapped": true,
            }),
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("create a tapped Treasure token");

    let treasure = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.name == "Treasure")
        .expect("Treasure enters the battlefield");
    assert!(treasure.tapped);
    assert!(treasure.definition.is_token);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn map_token_pays_taps_and_sacrifices_before_target_creature_explores() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    let mut map_definition = test_definition("token:map", "Artifact - Map");
    map_definition.name = "Map".to_string();
    map_definition.is_token = true;
    map_definition.is_game_piece = true;
    map_definition.power = None;
    map_definition.toughness = None;
    map_definition.rules = named_token_rules("Map").expect("Map rules");
    engine.state.players[0].battlefield = vec![
        test_instance("map", map_definition, "player-0"),
        test_instance(
            "explorer",
            test_definition("explorer", "Creature - Scout"),
            "player-0",
        ),
        mana_source("wastes", "Basic Land - Wastes", "{C}", None),
    ];
    engine.state.players[0].library = vec![test_instance(
        "explored-card",
        test_definition("explored-card", "Instant"),
        "player-0",
    )];

    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("map")
                && action.targets.get("targetCreature")
                    == Some(&TargetRef::Permanent {
                        instance_id: "explorer".to_string(),
                    })
        })
        .expect("Map explore action");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("activate Map");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|card| card.instance_id != "map")
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|card| card.instance_id == "wastes")
            .is_some_and(|card| card.tapped)
    );
    assert_eq!(engine.state.stack.len(), 1);

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("resolve explore");
    let explorer = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "explorer")
        .expect("exploring creature remains");
    assert_eq!(explorer.counters.get("+1/+1"), Some(&1));
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "explored-card")
    );
}
