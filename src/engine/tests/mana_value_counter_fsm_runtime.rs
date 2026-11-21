use super::*;
use crate::oracle::RecognizerProvenance;

fn parsed_disdainful_stroke_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Disdainful Stroke".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some("Counter target spell with mana value 4 or greater.".to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("native mana-value counter emits a rule");
    assert!(rule_is_executable(&rule));
    rule
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn parsed_mana_value_counter_offers_only_legal_threshold_targets_and_counters_one() {
    let rule = parsed_disdainful_stroke_rule();
    let mut engine = test_engine(2);
    let mut definition = test_definition("disdainful-stroke", "Instant");
    definition.name = "Disdainful Stroke".to_string();
    definition.mana_cost = "{1}{U}".to_string();
    definition.rules = vec![rule];
    engine.state.players[0].hand = vec![test_instance("disdainful-stroke", definition, "player-0")];
    engine.state.players[0].battlefield = vec![
        mana_source("island-1", "Basic Land - Island", "{U}", None),
        mana_source("island-2", "Basic Land - Island", "{U}", None),
    ];

    let mut cheap_definition = test_definition("cheap-spell", "Instant");
    cheap_definition.mana_cost = "{2}{U}".to_string();
    let mut expensive_definition = test_definition("expensive-spell", "Creature");
    expensive_definition.mana_cost = "{3}{G}".to_string();
    engine.state.stack = vec![
        StackObject {
            id: "stack:cheap".to_string(),
            controller: "player-1".to_string(),
            card: test_instance("cheap-spell", cheap_definition, "player-1"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: None,
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        },
        StackObject {
            id: "stack:expensive".to_string(),
            controller: "player-1".to_string(),
            card: test_instance("expensive-spell", expensive_definition, "player-1"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: None,
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        },
    ];

    let actions = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("disdainful-stroke")
        })
        .collect::<Vec<_>>();
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].targets.get("targetSpell"),
        Some(&TargetRef::StackObject {
            stack_id: "stack:expensive".to_string(),
        })
    );

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&actions[0], &mut RandomAi::seeded(18))
        .expect("cast Disdainful Stroke");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(18))
        .expect("resolve Disdainful Stroke");

    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].id, "stack:cheap");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "expensive-spell")
    );
}
