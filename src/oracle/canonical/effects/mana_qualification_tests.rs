use crate::card_catalog::named_token_printing;
use crate::oracle::canonical::compile_named_token_catalog_rules;

/// The single rule a token's Oracle compiles to through the shared grammar.
fn shared_token_rule(oracle: &str) -> Option<serde_json::Value> {
    let mut rules =
        compile_named_token_catalog_rules("Treasure", "Token Artifact - Treasure", oracle)?;
    (rules.len() == 1).then(|| rules.remove(0))
}

#[test]
fn catalog_ability_roles_follow_typed_execution_and_target_facts() {
    for (name, expected_kind) in [
        ("treasure", "manaAbility"),
        ("blood", "activatedAbility"),
        ("clue", "activatedAbility"),
        ("food", "activatedAbility"),
        ("map", "activatedAbility"),
    ] {
        let printing = named_token_printing(name).unwrap().unwrap();
        let rules = compile_named_token_catalog_rules(
            name,
            printing.type_line.as_deref().unwrap_or("Token Artifact"),
            printing.oracle_text.as_deref().unwrap(),
        )
        .unwrap_or_else(|| panic!("{name} compiles through the shared grammar"));
        assert_eq!(rules.len(), 1, "{name}");
        let rule = &rules[0];
        assert_eq!(rule["kind"], expected_kind, "{name}");
        if name == "map" {
            assert!(rule.get("declaration").is_some(), "{rule:#}");
        }
    }
}

#[test]
fn unnamed_mana_output_quantity_and_timing_are_preserved() {
    let rule = shared_token_rule(
        "Sacrifice this token: Add two mana of any color. Activate only as a sorcery.",
    )
    .unwrap();
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(
        rule["effects"][0]["mana"]["amount"],
        serde_json::json!({"kind":"integer","value":2})
    );
    assert_eq!(rule["costs"][0]["kind"], "sacrificePermanent");
    assert_eq!(rule["activationCondition"]["kind"], "sorceryTiming");
    assert!(rule.get("declaration").is_none());
    // CR 605.1a: an untargeted, nonloyalty ability that could add mana is a
    // mana ability even with another instruction; whatever the shared
    // grammar reads, it never drops the draw.
    if let Some(rule) =
        shared_token_rule("Sacrifice this token: Add two mana of any color. Draw a card.")
    {
        assert!(rule.to_string().contains("drawCards"), "{rule:#}");
    }
}
