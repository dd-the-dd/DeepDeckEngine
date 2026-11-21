use super::*;

const ORACLE_TEXT: &str = "Whenever one or more creatures you control become tapped, draw a card, then discard a card and put a plan counter on this enchantment.\nWhen the fourth plan counter is put on this enchantment, sacrifice it. When you do, return up to two target instant and/or sorcery cards from your graveyard to your hand.";

fn plan_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Rewrite History".to_string(),
        type_line: "Enchantment — Plan".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("test-plan", "Enchantment — Plan");
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical plan ability"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Rewrite History; see docs/coverage-targets.md"
)]
#[test]
fn controlled_creature_tap_resolves_draw_discard_then_plan_counter() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("test-plan", plan_definition(), "player-0"),
        test_instance(
            "tapping-creature",
            test_definition("tapping-creature", "Creature — Hero"),
            "player-0",
        ),
    ];
    engine.state.players[0].hand = vec![test_instance(
        "old-hand-card",
        test_definition("old-hand-card", "Land"),
        "player-0",
    )];
    engine.state.players[0].library = vec![test_instance(
        "drawn-card",
        test_definition("drawn-card", "Instant"),
        "player-0",
    )];

    let tapped = engine
        .tap_permanent("tapping-creature")
        .expect("the creature can tap")
        .expect("the creature was untapped");
    engine.enqueue_permanent_tapped_triggers(&tapped);
    assert_eq!(engine.state.stack.len(), 1);

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the tap trigger resolves from primitive effects");

    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|permanent| permanent.instance_id == "test-plan")
            .and_then(|permanent| permanent.counters.get("plan")),
        Some(&1)
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardDrawn" && event.card_instance_id.as_deref() == Some("drawn-card")
    }));
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "cardDiscarded")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn grouped_tap_memory_contains_only_permanents_matching_the_event_filter() {
    let mut source_definition = test_definition("tap-watcher", "Enchantment");
    source_definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "controlledPermanentsTapped",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": 1 },
        }],
    })];
    assert!(source_definition.rules.iter().all(rule_is_executable));

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("tap-watcher", source_definition, "player-0"),
        test_instance(
            "group-creature",
            test_definition("group-creature", "Creature — Hero"),
            "player-0",
        ),
        test_instance(
            "group-artifact",
            test_definition("group-artifact", "Artifact"),
            "player-0",
        ),
    ];
    let stack_object = StackObject {
        id: "stack:group-tap".to_string(),
        controller: "player-0".to_string(),
        card: engine.state.players[0].battlefield[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effect(
            &json!({
                "kind": "tapPermanents",
                "where": {
                    "kind": "or",
                    "operands": [
                        { "kind": "cardTypeContains", "value": "Creature" },
                        { "kind": "cardTypeContains", "value": "Artifact" },
                    ],
                },
            }),
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the group tap effect resolves as one event batch");

    let trigger = engine.state.stack.last().expect("one grouped trigger");
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(trigger.decisions["triggeringPermanentCount"], 1);
    assert_eq!(
        trigger.decisions["triggeringPermanentIds"],
        json!(["group-creature"])
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Rewrite History; see docs/coverage-targets.md"
)]
#[test]
fn fourth_plan_counter_sacrifices_source_then_returns_two_spells_reflexively() {
    let mut plan = test_instance("test-plan", plan_definition(), "player-0");
    plan.counters.insert("plan".to_string(), 3);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(plan);
    engine.state.players[0].graveyard = vec![
        test_instance(
            "graveyard-instant",
            test_definition("graveyard-instant", "Instant"),
            "player-0",
        ),
        test_instance(
            "graveyard-creature",
            test_definition("graveyard-creature", "Creature — Hero"),
            "player-0",
        ),
        test_instance(
            "graveyard-sorcery",
            test_definition("graveyard-sorcery", "Sorcery"),
            "player-0",
        ),
    ];
    let initial_hand_size = engine.state.players[0].hand.len();

    assert_eq!(engine.put_counters_on_permanent("test-plan", "plan", 1), 1);
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the fourth counter triggers once"
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the ordinal counter trigger resolves");

    assert!(engine.permanent_position("test-plan").is_none());
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the reflexive trigger is created"
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the reflexive return resolves");

    let hand_ids = engine.state.players[0]
        .hand
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(engine.state.players[0].hand.len(), initial_hand_size + 2);
    assert!(hand_ids.contains("graveyard-instant"));
    assert!(hand_ids.contains("graveyard-sorcery"));
    assert!(!hand_ids.contains("graveyard-creature"));
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "graveyard-creature")
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "test-plan")
    );
}
