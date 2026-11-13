use crate::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse(text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generalized Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn terminate_lowers_to_one_modified_destroy_primitive() {
    let parsed = parse("Destroy target creature. It can't be regenerated.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native rule");
    assert_eq!(rule["kind"], "spellAbility");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 1);
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][0]["cannotRegenerate"], true);
}

#[test]
fn alternate_result_anaphor_uses_the_same_primitive_lowering() {
    let parsed =
        parse("Destroy target creature. A creature destroyed this way can't be regenerated.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["kind"],
        "destroyPermanent"
    );
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["cannotRegenerate"],
        true
    );
}

#[test]
fn sibling_changes_the_selector_without_new_routing() {
    let parsed = parse("Destroy target nonartifact creature. It can't be regenerated.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["cannotRegenerate"],
        true
    );
}

#[test]
fn malformed_amendment_remains_claimed_with_structured_failure() {
    let parsed = parse("Destroy target creature. It can't be restored.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "unsupported", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let diagnostic = ability.diagnostics.first().expect("structured failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("destroy-regeneration-prohibition")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("sequence.destroy-regeneration.regeneration")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("restored"));
}
