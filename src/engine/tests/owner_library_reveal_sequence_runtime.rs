use super::*;

fn owner_library_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Chaos Warp".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(
            "The owner of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("owner-library-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("owner-library sequence should parse");
    assert!(rule_is_executable(&rule));
    rule
}

fn resolve_owner_library_sequence(library: Vec<CardInstance>) -> GameEngine {
    let rule = owner_library_rule();
    let mut borrowed = test_instance(
        "borrowed-permanent",
        test_definition("borrowed-permanent", "Artifact"),
        "player-1",
    );
    borrowed.controller = "player-0".to_string();

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![borrowed];
    engine.state.players[0].library = vec![test_instance(
        "controller-library-sentinel",
        test_definition("controller-library-sentinel", "Sorcery"),
        "player-0",
    )];
    engine.state.players[1].library = library;
    let stack_object = StackObject {
        id: "stack:owner-library-reveal".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "owner-library-spell",
            test_definition("owner-library-spell", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "borrowed-permanent".to_string(),
            },
        )]),
    };
    let mut provider = EmeritusDecisionProvider;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("owner-bound primitive sequence resolves");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn target_owner_binding_survives_the_zone_change_and_controls_the_revealed_permanent() {
    let owner_library = (0..16)
        .map(|index| {
            let id = format!("owner-creature-{index}");
            test_instance(&id, test_definition(&id, "Creature"), "player-1")
        })
        .collect();
    let engine = resolve_owner_library_sequence(owner_library);

    assert!(engine.state.players[0].battlefield.is_empty());
    assert_eq!(
        engine.state.players[0]
            .library
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        vec!["controller-library-sentinel"]
    );
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
    assert_eq!(
        engine.state.players[1].battlefield[0].controller,
        "player-1"
    );
    assert_eq!(engine.state.players[1].library.len(), 16);
    let revealed_id = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "cardsRevealed")
        .and_then(|event| event.detail["cardIds"].as_array())
        .and_then(|ids| ids.first())
        .and_then(Value::as_str)
        .expect("one top card should be revealed");
    assert_eq!(
        engine.state.players[1].battlefield[0].instance_id,
        revealed_id
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "zoneShuffled" && event.player_id.as_deref() == Some("player-1")
    }));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_revealed_nonpermanent_stays_on_top_of_the_owners_library() {
    let owner_library = (0..100)
        .map(|index| {
            let id = format!("owner-instant-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-1")
        })
        .collect();
    let engine = resolve_owner_library_sequence(owner_library);

    assert!(engine.state.players[0].battlefield.is_empty());
    assert!(engine.state.players[1].battlefield.is_empty());
    assert_eq!(engine.state.players[1].library.len(), 101);
    let revealed_id = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "cardsRevealed")
        .and_then(|event| event.detail["cardIds"].as_array())
        .and_then(|ids| ids.first())
        .and_then(Value::as_str)
        .expect("one top card should be revealed");
    let top = engine.state.players[1]
        .library
        .last()
        .expect("the nonpermanent remains on top");
    assert_eq!(top.instance_id, revealed_id);
    assert!(has_card_type_name(&top.definition.type_line, "Instant"));
}
