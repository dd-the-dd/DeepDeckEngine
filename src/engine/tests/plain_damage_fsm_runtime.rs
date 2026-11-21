use super::*;

fn lightning_bolt_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Lightning Bolt".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some("Lightning Bolt deals 3 damage to any target.".to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("plain-damage-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("Lightning Bolt emits a canonical rule");
    assert!(rule_is_executable(&rule));
    rule
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_lightning_bolt_deals_three_damage_to_the_single_chosen_target() {
    let rule = lightning_bolt_rule();
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");

    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:lightning-bolt".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "lightning-bolt",
            test_definition("lightning-bolt", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetDamageable".to_string(),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    };

    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the parsed primitive damage resolves");

    assert_eq!(engine.state.players[0].life, 20);
    assert_eq!(engine.state.players[1].life, 17);
    let damage = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "damageDealt")
        .expect("damage event is recorded");
    assert_eq!(damage.player_id.as_deref(), Some("player-1"));
    assert_eq!(damage.detail["amount"], 3);
}
