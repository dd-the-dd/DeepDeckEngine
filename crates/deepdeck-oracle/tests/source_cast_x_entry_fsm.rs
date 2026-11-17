use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ENTRY_ORACLE: &str =
    "When Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down.";

fn parse_entry(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Legendary Creature — Bird Spirit".to_string(),
        mana_cost: Some("{X}{U}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn wan_shi_tong_entry_is_native_and_preserves_both_numeric_expressions() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/wan-shi-tong-entry.json"
    ))
    .expect("Wan Shi Tong entry ground truth is valid JSON");
    assert_eq!(truth["context"]["oracleText"], ENTRY_ORACLE);

    let parsed = parse_entry("Wan Shi Tong, Librarian", ENTRY_ORACLE);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let lexical_name_tokens = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert_eq!(lexical_name_tokens, ["Wan", "Shi", "Tong"]);
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&truth["abilities"][0]["expectedRule"])
    );
}

#[test]
fn another_source_counter_kind_and_optional_of_reuse_the_same_children() {
    let parsed = parse_entry(
        "Test Sage",
        "When this creature enters, put X -1/-1 counters on it. Then draw half of X cards, rounded down.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().unwrap();
    assert_eq!(rule["effects"][0]["counter"], "-1/-1");
    assert_eq!(
        rule["effects"][0]["count"],
        json!({ "kind": "sourceCastXValue" })
    );
    assert_eq!(
        rule["effects"][1]["count"],
        json!({
            "kind": "divide",
            "left": { "kind": "sourceCastXValue" },
            "right": { "kind": "integer", "value": 2 },
            "round": "down",
        })
    );
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_parent_or_numeric_child() {
    for (oracle_text, state) in [
        (
            "When Test Sage enters, put X +1/+1 counters on it and draw half X cards, rounded down.",
            "source-cast-x-entry.sequence-shape",
        ),
        (
            "When Test Sage enters, put X +1/+1 counters on target creature. Then draw half X cards, rounded down.",
            "source-cast-x-entry.sequence-shape",
        ),
        (
            "When Test Sage enters, put X +1/+1 counters on it. Then draw half X cards, rounded up.",
            "effect.draw-cards.rounding-direction",
        ),
        (
            "When Test Sage enters, put X +1/+1 counters on it. Then draw X cards.",
            "source-cast-x-entry.sequence-shape",
        ),
    ] {
        let parsed = parse_entry("Test Sage", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert!(ability.rule.is_none());
        // The shared trigger program reports its own committed failure first;
        // the family's precise near-miss state stays visible as evidence.
        assert!(
            ability.diagnostics.iter().any(|diagnostic| {
                diagnostic.automaton.as_deref() == Some("source-cast-x-entry")
                    && diagnostic.state.as_deref() == Some(state)
            }),
            "{oracle_text}: {:#?}",
            ability.diagnostics
        );
    }
}
