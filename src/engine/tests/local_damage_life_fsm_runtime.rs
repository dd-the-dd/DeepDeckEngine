use super::*;

fn lightning_helix_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Lightning Helix".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(
            "Lightning Helix deals 3 damage to any target and you gain 3 life.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("local-damage-life-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("Lightning Helix emits a canonical rule");
    assert!(rule_is_executable(&rule));
    rule
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_damage_executes_before_the_independent_life_gain() {
    let rule = lightning_helix_rule();
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][1]["kind"], "gainLife");

    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:lightning-helix".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "lightning-helix",
            test_definition("lightning-helix", "Instant"),
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
        .expect("the parsed primitive sequence resolves");

    assert_eq!(engine.state.players[1].life, 17);
    assert_eq!(engine.state.players[0].life, 23);

    let damage = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "damageDealt")
        .expect("damage event is recorded");
    let life_gain = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "lifeGained")
        .expect("life-gain event is recorded");
    assert!(damage.sequence < life_gain.sequence);
    assert_eq!(damage.player_id.as_deref(), Some("player-1"));
    assert_eq!(damage.detail["amount"], 3);
    assert_eq!(life_gain.player_id.as_deref(), Some("player-0"));
    assert_eq!(life_gain.detail["amount"], 3);
}
