//! Real Oracle text -> counter placement trigger IR -> engine behavior: one
//! placement of a named counter kind on one permanent is one event (CR 122.6),
//! with the recipient (the source or a permanent query) and the placing player
//! as independent attributes.
use super::*;

fn observer(instance_id: &str, owner: &str, type_line: &str, oracle: &str) -> CardInstance {
    let request = OracleCardParseRequest {
        card_name: instance_id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    let mut definition = test_definition(instance_id, type_line);
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect();
    test_instance(instance_id, definition, owner)
}

fn triggers_from(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| {
            event.kind == "triggeredAbilityPutOnStack"
                && event.card_instance_id.as_deref() == Some(instance_id)
        })
        .count()
}

fn place(engine: &mut GameEngine, instance_id: &str, counter: &str, placed_by: &str) {
    engine
        .place_counters(
            instance_id,
            counter,
            2,
            QuantityOrigin::RuleAction,
            Some(placed_by),
            &mut EmeritusDecisionProvider,
        )
        .expect("counters are placed");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn counter_placements_match_kind_recipient_and_placer() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        // Herd Baloth's event with an unused body.
        observer(
            "baloth",
            "player-0",
            "Creature — Beast",
            "Whenever one or more +1/+1 counters are put on this creature, you gain 1 life.",
        ),
        // Wickersmith's Tools' event: any controller's creature.
        observer(
            "tools",
            "player-0",
            "Artifact",
            "Whenever one or more -1/-1 counters are put on a creature, you gain 2 life.",
        ),
        // Unused combination: the controller as placer, another creature.
        observer(
            "placer",
            "player-0",
            "Creature — Elf",
            "Whenever you put one or more +1/+1 counters on another creature you control, you gain 4 life.",
        ),
        test_instance(
            "own-bear",
            test_definition("own-bear", "Creature — Bear"),
            "player-0",
        ),
    ];
    engine.state.players[1].battlefield = vec![test_instance(
        "their-bear",
        test_definition("their-bear", "Creature — Bear"),
        "player-1",
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    place(&mut engine, "baloth", "+1/+1", "player-0");
    assert_eq!(
        triggers_from(&engine, "baloth"),
        1,
        "one placement, two counters"
    );
    assert_eq!(triggers_from(&engine, "placer"), 1, "another creature");
    place(&mut engine, "placer", "+1/+1", "player-0");
    assert_eq!(triggers_from(&engine, "placer"), 1, "not the source itself");
    place(&mut engine, "own-bear", "+1/+1", "player-0");
    assert_eq!(triggers_from(&engine, "baloth"), 1, "another recipient");
    assert_eq!(triggers_from(&engine, "placer"), 2);
    place(&mut engine, "own-bear", "+1/+1", "player-1");
    assert_eq!(
        triggers_from(&engine, "placer"),
        2,
        "an opponent placed them"
    );
    place(&mut engine, "their-bear", "-1/-1", "player-1");
    assert_eq!(triggers_from(&engine, "tools"), 1, "any controller");
    place(&mut engine, "their-bear", "+1/+1", "player-1");
    assert_eq!(triggers_from(&engine, "tools"), 1, "another counter kind");
}
