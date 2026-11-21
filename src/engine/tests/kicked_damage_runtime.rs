use super::*;

fn burst_lightning_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Burst Lightning".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some(
            "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("kicked-damage-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical");
    let rule = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(" deals "))
        .and_then(|ability| ability.rule.clone())
        .expect("Burst Lightning's damage rule parses");
    assert!(rule_is_executable(&rule));
    rule
}

fn resolve_burst_lightning(rule: &Value, kicked: bool) -> GameEngine {
    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: format!("stack:burst-lightning:{kicked}"),
        controller: "player-0".to_string(),
        card: test_instance(
            "burst-lightning",
            test_definition("burst-lightning", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::from([("kicked".to_string(), Value::Bool(kicked))]),
        targets: BTreeMap::from([(
            "targetDamageable".to_string(),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    };
    let mut decisions = stack_object.decisions.clone();
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut EmeritusDecisionProvider,
        )
        .expect("the kicked damage branches resolve");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_same_rule_deals_the_base_or_kicked_amount_to_the_single_chosen_target() {
    let rule = burst_lightning_rule();

    let unkicked = resolve_burst_lightning(&rule, false);
    assert_eq!(unkicked.state.players[1].life, 18);

    let kicked = resolve_burst_lightning(&rule, true);
    assert_eq!(kicked.state.players[1].life, 16);
}
