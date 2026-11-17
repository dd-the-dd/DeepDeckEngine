use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Each creature with mana value X or less loses all abilities until end of turn. Destroy those creatures.";

fn parse(oracle_text: &str, mana_cost: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Set Effect Probe".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn variable_threshold_ability_loss_then_destruction_is_native_and_primitive() {
    let parsed = parse(EXACT_ORACLE, "{X}{B}{B}");
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );

    let rule = ability.rule.as_ref().expect("primitive rule");
    assert_eq!(
        rule["declaration"]["decisions"],
        json!([{ "id": "xValue", "kind": "chooseNumber", "minimum": 0 }])
    );
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][0]["id"], "affectedPermanents");
    assert_eq!(
        rule["effects"][0]["value"]["where"]["operands"][1]["right"],
        json!({ "kind": "decisionResult", "decisionId": "xValue" })
    );
    assert_eq!(rule["effects"][1]["kind"], "forEachObject");
    assert_eq!(
        rule["effects"][1]["effects"][0]["kind"],
        "installLoseAllAbilities"
    );
    assert_eq!(rule["effects"][2]["kind"], "destroyPermanent");
    assert_eq!(
        rule["effects"][2]["permanent"],
        json!({ "kind": "boundObjects", "binding": "affectedPermanents" })
    );
}

#[test]
fn literal_threshold_and_permanent_type_are_independent_children() {
    let parsed = parse(
        "Each artifact with mana value three or less loses all abilities until end of turn. Destroy those artifacts.",
        "{2}{B}",
    );
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().unwrap();
    assert!(rule.get("declaration").is_none());
    assert_eq!(
        rule["effects"][0]["value"]["where"]["operands"][0],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        rule["effects"][0]["value"]["where"]["operands"][1]["right"],
        json!({ "kind": "integer", "value": 3 })
    );
}

#[test]
fn mismatched_anaphor_and_unsupported_duration_fail_after_family_commit() {
    let mismatch = parse(
        "Each creature with mana value X or less loses all abilities until end of turn. Destroy those artifacts.",
        "{X}{B}",
    );
    assert_eq!(mismatch.status, "unsupported");
    assert_eq!(
        mismatch.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        mismatch.abilities[0].diagnostics[0].automaton.as_deref(),
        Some("effect-sequence")
    );
    assert_eq!(
        mismatch.abilities[0].diagnostics[0].state.as_deref(),
        Some("reference.object-set.validation")
    );

    let duration = parse(
        "Each creature with mana value X or less loses all abilities until your next turn. Destroy those creatures.",
        "{X}{B}",
    );
    assert_eq!(duration.status, "unsupported");
    assert_eq!(
        duration.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        duration.abilities[0].diagnostics[0].state.as_deref(),
        Some("ability-loss-destroy.lowering.duration")
    );
}
