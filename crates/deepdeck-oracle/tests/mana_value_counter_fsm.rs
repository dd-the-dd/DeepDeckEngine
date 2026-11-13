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
fn disdainful_stroke_is_native_and_matches_its_exact_ground_truth_fixture() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/disdainful-stroke.json"
    ))
    .expect("Disdainful Stroke ground-truth fixture is valid JSON");
    let context = &truth["context"];
    assert_eq!(context["typeLine"], "Instant");
    assert_eq!(context["manaCost"], "{1}{U}");
    assert_eq!(
        context["oracleText"],
        "Counter target spell with mana value 4 or greater."
    );

    let parsed = parse_counter(
        context["name"].as_str().expect("fixture card name"),
        context["oracleText"].as_str().expect("fixture Oracle text"),
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
fn spell_snare_is_native_and_matches_its_exact_ground_truth_fixture() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/spell-snare.json"
    ))
    .expect("Spell Snare ground-truth fixture is valid JSON");
    let context = &truth["context"];
    assert_eq!(context["typeLine"], "Instant");
    assert_eq!(context["manaCost"], "{U}");
    assert_eq!(
        context["oracleText"],
        "Counter target spell with mana value 2."
    );

    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: context["name"]
            .as_str()
            .expect("fixture card name")
            .to_string(),
        type_line: context["typeLine"]
            .as_str()
            .expect("fixture type line")
            .to_string(),
        mana_cost: Some(
            context["manaCost"]
                .as_str()
                .expect("fixture mana cost")
                .to_string(),
        ),
        oracle_text: Some(
            context["oracleText"]
                .as_str()
                .expect("fixture Oracle text")
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(truth["abilities"][0]["expectedRule"].clone())
    );
}

#[test]
fn literal_word_and_equality_siblings_reuse_the_numeric_selector() {
    for (oracle_text, operator, value) in [
        (
            "Counter target spell with mana value three or greater.",
            ">=",
            3,
        ),
        ("Counter target spell with mana value 4 or less.", "<=", 4),
        ("Counter target spell with mana value 2.", "==", 2),
    ] {
        let parsed = parse_counter("Threshold Counter", oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let predicate = &parsed.abilities[0].rule.as_ref().expect("primitive rule")["declaration"]
            ["decisions"][0]["candidates"]["where"];
        assert_eq!(predicate["kind"], "compare");
        assert_eq!(predicate["operator"], operator);
        assert_eq!(predicate["left"]["kind"], "manaValueOf");
        assert_eq!(
            predicate["right"],
            json!({ "kind": "integer", "value": value })
        );
    }
}

#[test]
fn unknown_comparison_direction_is_a_committed_native_failure() {
    let parsed = parse_counter(
        "Threshold Counter",
        "Counter target spell with mana value 4 or lower.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("mana-value-counter"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("selector.numeric-comparison.direction")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("lower"));
}

#[test]
fn unknown_threshold_is_reported_at_the_numeric_child_without_fallback() {
    let parsed = parse_counter(
        "Threshold Counter",
        "Counter target spell with mana value heroic or greater.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert!(ability.entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("heroic") && entity.token_type == "unknown"
    }));
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("mana-value-counter"));
    assert_eq!(diagnostic.state.as_deref(), Some("numeric.atom.start"));
    assert_eq!(diagnostic.found.as_deref(), Some("heroic"));
}

#[test]
fn missing_terminator_is_a_committed_native_failure() {
    let parsed = parse_counter(
        "Threshold Counter",
        "Counter target spell with mana value 4 or greater",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("mana-value-counter"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("sequence.connector-or-end")
    );
    assert!(diagnostic.found.is_none());
}

#[test]
fn promoted_mana_value_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_counter(
        "Disdainful Stroke",
        "Counter target spell with mana value 4 or greater.",
    );
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
