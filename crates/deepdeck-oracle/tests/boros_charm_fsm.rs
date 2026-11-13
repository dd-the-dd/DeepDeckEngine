use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

const BOROS_CHARM_ORACLE: &str = "Choose one \u{2014}\n\u{2022} Boros Charm deals 4 damage to target player or planeswalker.\n\u{2022} Permanents you control gain indestructible until end of turn.\n\u{2022} Target creature gains double strike until end of turn.";

fn parse(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn boros_charm_is_native_modal_primitive_ir() {
    let parsed = parse("Boros Charm", BOROS_CHARM_ORACLE);

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native modal rule");
    assert_eq!(rule["kind"], "spellAbility");
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseModes");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"]["value"], 1);
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"]["kind"],
        "union"
    );
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"]["sets"][0]["kind"],
        "players"
    );
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"]["sets"][1]["kind"],
        "permanents"
    );
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(3));
    assert_eq!(rule["effects"][0]["then"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][0]["then"][0]["amount"]["value"], 4);
    assert_eq!(rule["effects"][1]["then"][0]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][1]["then"][0]["keyword"], "indestructible");
    assert_eq!(rule["effects"][2]["then"][0]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][2]["then"][0]["keyword"], "doubleStrike");
}

#[test]
fn modal_sibling_varies_value_cross_domain_target_set_and_keywords() {
    let parsed = parse(
        "Test Charm",
        "Choose one \u{2014}\n\u{2022} Test Charm deals 2 damage to target opponent or creature.\n\u{2022} Artifacts you control gain hexproof until end of turn.\n\u{2022} Target artifact gains flying until end of turn.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(rule["effects"][0]["then"][0]["amount"]["value"], 2);
    assert_eq!(rule["effects"][1]["then"][0]["keyword"], "hexproof");
    assert_eq!(rule["effects"][2]["then"][0]["keyword"], "flying");
}

#[test]
fn modal_damage_rejects_a_nondamageable_union_branch() {
    let parsed = parse(
        "Test Charm",
        "Choose one \u{2014}\n\u{2022} Test Charm deals 2 damage to target player or spell.\n\u{2022} Permanents you control gain indestructible until end of turn.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostics =
        serde_json::to_string(&parsed.abilities[0].diagnostics).expect("diagnostics serialize");
    assert!(diagnostics.contains("damageable"));
}

#[test]
fn promoted_modal_rejects_trailing_unconsumed_tokens() {
    let parsed = parse("Boros Charm", &format!("{BOROS_CHARM_ORACLE} Draw a card."));

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostics =
        serde_json::to_string(&parsed.abilities[0].diagnostics).expect("diagnostics serialize");
    assert!(diagnostics.contains("end of input"));
}
