use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Counter".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn simple_spell_counters_use_one_native_target_and_one_primitive_effect() {
    for (oracle_text, expected_filter) in [
        (
            "Counter target noncreature spell.",
            Some(json!({
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Creature" },
            })),
        ),
        ("Counter target spell.", None),
        (
            "Counter target nonartifact spell.",
            Some(json!({
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Artifact" },
            })),
        ),
        (
            "Counter target creature spell.",
            Some(json!({ "kind": "cardTypeContains", "value": "Creature" })),
        ),
        (
            "Counter target hero spell.",
            Some(json!({ "kind": "subtypeContains", "value": "Hero" })),
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
        let candidates = &rule["declaration"]["decisions"][0]["candidates"];
        assert_eq!(candidates["kind"], "spells");
        match expected_filter {
            Some(filter) => assert_eq!(candidates["where"], filter),
            None => assert!(candidates.get("where").is_none()),
        }
        assert_eq!(rule["effects"][0]["kind"], "counterSpell");
        assert_eq!(rule["effects"][0]["spell"]["id"], "targetSpell");
    }
}

#[test]
fn claimed_unknown_qualifier_reports_the_selector_state_without_legacy_fallback() {
    let parsed = parse_spell("Counter target unimplementedqualifierzz spell.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("simple-counter"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("selector.predicate.start")
    );
    assert_eq!(
        diagnostic.found.as_deref(),
        Some("unimplementedqualifierzz")
    );
}

#[test]
fn payment_counter_family_uses_its_dedicated_native_boundary() {
    let parsed = parse_spell("Counter target noncreature spell unless its controller pays {2}.");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().unwrap();
    assert_eq!(rule["effects"][0]["kind"], "tryPayCost");
    assert_eq!(
        rule["effects"][0]["cost"],
        json!({ "kind": "payMana", "manaCost": "{2}" })
    );
    assert_eq!(
        rule["effects"][1]["condition"],
        json!({
            "kind": "costWasPaid", "decisionId": "resolutionPayment0", "value": false, "required": true,
        })
    );
    assert_eq!(
        rule["effects"][1]["then"],
        json!([{
            "kind": "counterStackObject", "object": { "kind": "chosenTarget", "id": "targetSpell" },
        }])
    );
}
