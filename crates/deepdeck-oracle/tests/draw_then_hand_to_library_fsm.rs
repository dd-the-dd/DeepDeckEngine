use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Brainstorm".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn brainstorm_is_native_and_lowers_to_independent_primitives() {
    let parsed = parse_instant(
        "Draw three cards, then put two cards from your hand on top of your library in any order.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the native sequence emits a rule");
    let effects = rule["effects"]
        .as_array()
        .expect("ordered primitive effects");

    assert_eq!(effects.len(), 4);
    assert_eq!(effects[0]["kind"], "drawCards");
    assert_eq!(
        effects[0]["count"],
        json!({ "kind": "integer", "value": 3 })
    );
    assert_eq!(effects[1]["kind"], "chooseCards");
    assert_eq!(
        effects[1]["minimum"],
        json!({ "kind": "integer", "value": 2 })
    );
    assert_eq!(
        effects[1]["maximum"],
        json!({ "kind": "integer", "value": 2 })
    );
    assert_eq!(effects[1]["candidates"]["zone"]["kind"], "hand");
    assert_eq!(effects[2]["kind"], "chooseOrder");
    assert_eq!(effects[3]["kind"], "moveCards");
    assert_eq!(effects[3]["to"]["kind"], "library");
    assert_eq!(effects[3]["to"]["position"], "top");
    assert_eq!(
        effects[3]["order"],
        json!({
            "kind": "decisionOrder",
            "decisionId": "cardsForLibraryOrder",
        })
    );
    assert!(!rule.to_string().contains("drawThen"));
}

#[test]
fn fixed_and_variable_counts_reuse_numeric_and_collection_automata() {
    for (oracle_text, draw_count, move_count) in [
        (
            "Draw two cards, then put one card from your hand on top of your library in any order.",
            json!({ "kind": "integer", "value": 2 }),
            json!({ "kind": "integer", "value": 1 }),
        ),
        (
            "Draw X cards, then put X cards from your hand on top of your library in any order.",
            json!({ "kind": "decisionResult", "decisionId": "xValue" }),
            json!({ "kind": "decisionResult", "decisionId": "xValue" }),
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let rule = parsed.abilities[0].rule.as_ref().unwrap();
        assert_eq!(rule["effects"][0]["count"], draw_count, "{oracle_text}");
        assert_eq!(rule["effects"][1]["minimum"], move_count, "{oracle_text}");
        if oracle_text.contains(" X ") {
            assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseNumber");
        }
    }
}

#[test]
fn promoted_near_misses_fail_in_the_responsible_child_automaton() {
    for (oracle_text, state, found) in [
        (
            "Draw three cards, then put two cards from your graveyard on top of your library in any order.",
            "zone.possessive.kind",
            "graveyard",
        ),
        (
            "Draw one cards, then put one card from your hand on top of your library in any order.",
            "collection.card-noun",
            "cards",
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_ne!(ability.status, "canonical", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure");
        assert_eq!(diagnostic.automaton.as_deref(), Some("effect-sequence"));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_instant(
        "Draw three cards, then put two cards from your hand on top of your library in any order.",
    );
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
