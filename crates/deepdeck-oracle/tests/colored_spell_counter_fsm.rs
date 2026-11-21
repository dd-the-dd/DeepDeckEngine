use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_counter(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn flashfreeze_is_a_native_counter_with_one_union_filtered_spell_target() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/flashfreeze.json"
    ))
    .expect("Flashfreeze ground-truth fixture is valid JSON");
    let context = &truth["context"];
    let oracle_text = context["oracleText"]
        .as_str()
        .expect("ground truth has Oracle text");
    assert_eq!(context["typeLine"], "Instant");
    assert_eq!(context["manaCost"], "{1}{U}");
    assert_eq!(oracle_text, "Counter target red or green spell.");

    let parsed = parse_counter(
        context["name"]
            .as_str()
            .expect("ground truth has card name"),
        oracle_text,
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(truth["abilities"][0]["expectedRule"].clone())
    );
}

#[test]
fn another_color_union_reuses_the_same_spell_selector_automaton() {
    let parsed = parse_counter("Test Color Counter", "Counter target white or blue spell.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let candidates = &parsed.abilities[0].rule.as_ref().expect("primitive rule")["declaration"]["decisions"]
        [0]["candidates"];
    assert_eq!(candidates["kind"], "spells");
    assert_eq!(
        candidates["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "colorContains", "value": "White" },
                { "kind": "colorContains", "value": "Blue" },
            ],
        })
    );
}

#[test]
fn dangling_color_union_is_a_committed_native_failure() {
    let parsed = parse_counter("Test Color Counter", "Counter target red or spell.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("colored-spell-counter")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.counter.target-domain")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("."));
}

#[test]
fn colored_nonspell_noun_is_not_silently_accepted_by_legacy_fallback() {
    let parsed = parse_counter(
        "Test Color Counter",
        "Counter target red or green permanent.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("colored-spell-counter")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.counter.target-domain")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("."));
}

#[test]
fn promoted_color_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_counter("Flashfreeze", "Counter target red or green spell.");
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
