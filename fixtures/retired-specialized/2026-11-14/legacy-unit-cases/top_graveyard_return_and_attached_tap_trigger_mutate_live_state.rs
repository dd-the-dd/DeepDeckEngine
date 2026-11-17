#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn top_graveyard_return_and_attached_tap_trigger_mutate_live_state() {
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.extend([
        test_instance(
            "older-creature",
            test_definition("older-creature", "Creature"),
            "player-0",
        ),
        test_instance(
            "top-noncreature",
            test_definition("top-noncreature", "Sorcery"),
            "player-0",
        ),
        test_instance(
            "top-creature",
            test_definition("top-creature", "Creature"),
            "player-0",
        ),
    ]);
    let stack_object = StackObject {
        id: "stack:top-graveyard-return".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "return-source",
            test_definition("return-source", "Instant"),
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
        .execute_effects(
            &json!([{
                "kind": "moveTopMatchingCard",
                "zone": {
                    "kind": "graveyard",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": { "kind": "cardTypeContains", "value": "Creature" },
                "to": {
                    "kind": "battlefield",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "tapped": false,
                },
                "grantKeywords": ["haste"],
                "exileAtNextEndStep": true,
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RandomAi::seeded(41),
        )
        .expect("the top matching graveyard card returns");
    let returned = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "top-creature")
        .expect("the most recent matching graveyard card moved");
    assert!(has_keyword(&returned.definition, "haste"));
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("exileAtEndStep")
            && modifier["cardInstanceId"] == "top-creature"
    }));

    let creature = test_instance(
        "equipped-creature",
        test_definition("equipped-creature", "Creature"),
        "player-0",
    );
    let mut equipment = test_instance(
        "tap-observer",
        test_definition("tap-observer", "Artifact - Equipment"),
        "player-0",
    );
    equipment.attached_to = Some("equipped-creature".to_string());
    equipment.definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "attachedPermanentTapped",
            "attachment": { "kind": "self" },
        },
        "effects": [{
            "kind": "dealDamageToEachOpponent",
            "amount": { "kind": "integer", "value": 1 },
            "source": { "kind": "triggeringPermanent" },
        }],
    })];
    engine.state.players[0]
        .battlefield
        .extend([creature, equipment]);
    let tapped = engine
        .tap_permanent("equipped-creature")
        .expect("the equipped creature can tap")
        .expect("the permanent changed from untapped to tapped");
    engine.enqueue_permanent_tapped_triggers(&tapped);
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut RandomAi::seeded(43))
        .expect("the attached tap trigger resolves");
    assert_eq!(engine.state.players[1].life, 19);
}
