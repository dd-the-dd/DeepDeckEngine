use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, ParsedOracleAbility, parse_oracle_card};

const PLANESWALKER: &str = "Legendary Planeswalker — Probe";

fn parse(type_line: &str, text: &str) -> Vec<ParsedOracleAbility> {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Probe".into(),
        type_line: type_line.into(),
        mana_cost: None,
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: Vec::new(),
    })
    .abilities
}

fn rule(text: &str) -> Value {
    let abilities = parse(PLANESWALKER, text);
    assert_eq!(abilities.len(), 1, "{abilities:?}");
    assert_eq!(
        abilities[0].status, "canonical",
        "{:?}",
        abilities[0].diagnostics
    );
    abilities[0].rule.clone().unwrap()
}

fn envelope_is_loyalty(rule: &Value, delta: i64) {
    assert_eq!(rule["kind"], json!("activatedAbility"));
    assert_eq!(
        rule["costs"],
        json!([{"kind":"payLoyalty","object":{"kind":"self"},"amount":{"kind":"integer","value":delta}}])
    );
    assert_eq!(rule["activationCondition"], json!({"kind":"sorceryTiming"}));
    assert_eq!(
        rule["activationLimit"],
        json!({"kind":"oncePerTurn","id":"loyaltyAbility"})
    );
}

#[test]
fn loyalty_cost_composes_with_the_shared_effect_program() {
    // Every sign spelling and a previously unused combination of supported
    // actions: no loyalty-specific body recognizer is involved.
    for (text, delta, effects) in [
        ("−3: Draw a card.", -3, 1),
        ("-1: Surveil 1.", -1, 1),
        ("+1: Scry 2, then draw a card.", 1, 2),
        ("0: You gain 2 life. Draw a card.", 0, 2),
    ] {
        let rule = rule(text);
        envelope_is_loyalty(&rule, delta);
        assert!(
            rule["effects"].as_array().unwrap().len() >= effects,
            "{text}: {rule}"
        );
    }
}

#[test]
fn loyalty_ability_with_mana_effect_is_not_a_mana_ability() {
    envelope_is_loyalty(&rule("+1: Add {G}{G}."), 1);
}

#[test]
fn loyalty_ability_near_misses_fail_explicitly() {
    for text in [
        "−X: Draw a card.",
        "+1: Frobnicate the planeswalker.",
        "−1: Draw a card. Surveil",
    ] {
        let abilities = parse(PLANESWALKER, text);
        let last = abilities.last().unwrap();
        assert_ne!(last.status, "canonical", "{text}");
        assert!(last.rule.is_none(), "{text}");
        assert!(!last.diagnostics.is_empty(), "{text}");
    }
}

#[test]
fn token_loyalty_abilities_compile_through_the_named_token_catalog_path() {
    let rules = crate::oracle::compile_named_token_catalog_rules(
        "jace",
        "Token Legendary Planeswalker — Jace",
        "−1: Surveil 1.\n−3: Draw a card.",
    )
    .expect("token loyalty abilities compile");
    assert_eq!(rules.len(), 2);
    envelope_is_loyalty(&rules[0], -1);
    envelope_is_loyalty(&rules[1], -3);
}
