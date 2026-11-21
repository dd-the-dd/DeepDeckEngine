use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parsed_entry_rule(oracle_text: &str) -> serde_json::Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Reflection".to_string(),
        type_line: "Enchantment — Aura".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed.abilities[1]
        .rule
        .clone()
        .expect("canonical entry rule")
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Secret Invasion pattern; see docs/coverage-targets.md"
)]
#[test]
fn optional_linked_exile_and_attached_copy_are_executable_primitives() {
    let rule = parsed_entry_rule(
        "Enchant creature you control\nWhen this Aura enters, exile up to one target creature other than enchanted creature until this Aura leaves the battlefield. Enchanted creature becomes a copy of that creature until this Aura leaves the battlefield.\nEnchanted creature has ward {2}.",
    );

    assert!(rule_is_executable(&rule), "{rule:#}");
    let serialized = serde_json::to_string(&rule).expect("rule serializes");
    assert!(!serialized.contains("resolveTriggeredInstruction"));
    assert!(!serialized.contains("Secret Invasion"));
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Secret Invasion pattern; see docs/coverage-targets.md"
)]
#[test]
fn mandatory_artifact_variant_uses_the_same_executable_contract() {
    let rule = parsed_entry_rule(
        "Enchant artifact you control\nWhen this Aura enters, exile target artifact other than enchanted permanent until this Aura leaves the battlefield. Enchanted permanent becomes a copy of that permanent until this Aura leaves the battlefield.",
    );

    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert!(rule_is_executable(&rule), "{rule:#}");
}
