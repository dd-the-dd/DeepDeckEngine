use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

fn assert_all_rules_executable(oracle_text: &str, native_cost: bool) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    if native_cost {
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.recognizer == RecognizerProvenance::Native)
        );
    }
    for ability in parsed.abilities {
        let rule = ability
            .rule
            .expect("every ability should have primitive IR");
        assert!(rule_is_executable(&rule), "non-executable rule: {rule}");
        assert!(!rule.to_string().contains("resolve"));
    }
}

#[test]
fn discard_or_life_additional_cost_and_effect_are_executable() {
    assert_all_rules_executable(
        "As an additional cost to cast this spell, discard a card or pay 3 life.\nDestroy target creature or planeswalker.",
        true,
    );
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Bone Shards pattern; see docs/coverage-targets.md"
)]
#[test]
fn sacrifice_or_life_sibling_is_executable() {
    assert_all_rules_executable(
        "As an additional cost to cast this spell, sacrifice a creature or pay 4 life.\nDestroy target artifact.",
        false,
    );
}
