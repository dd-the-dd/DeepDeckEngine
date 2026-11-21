use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};
use serde_json::Value;

fn parse_damage_rule(card_name: &str, oracle_text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("kicked-damage-contract-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract test thread starts")
        .join()
        .expect("contract test thread completes");
    assert_eq!(parsed.status, "canonical");
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(" deals "))
        .and_then(|ability| ability.rule.clone())
        .expect("the kicked damage instruction should emit a rule")
}

#[test]
fn burst_lightning_is_executable_without_a_card_specific_operation() {
    let rule = parse_damage_rule(
        "Burst Lightning",
        "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(
        rule["declaration"]["decisions"].as_array().map(Vec::len),
        Some(1)
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("burstLightning"));
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("conditionalValue"));
}

#[test]
fn every_burst_lightning_ability_is_native_and_executable() {
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
        .name("native-kicker-contract-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract test thread starts")
        .join()
        .expect("contract test thread completes");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        serde_json::to_value(ability)
            .ok()
            .is_some_and(|value| value["recognizer"] == "native")
    }));
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| { ability.rule.as_ref().is_some_and(rule_is_executable) })
    );
}

#[test]
fn creature_target_sibling_uses_the_same_executable_branch_shape() {
    let rule = parse_damage_rule(
        "Test Spark",
        "Test Spark deals 3 damage to target artifact creature. If this spell was kicked, it deals 7 damage instead.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"][0]["kind"], "conditionalEffect");
    assert_eq!(rule["effects"][0]["then"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][0]["else"][0]["kind"], "dealDamage");
}
