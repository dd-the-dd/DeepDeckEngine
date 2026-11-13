use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "{2}{U}{B}: Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.";

fn parse_animation(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Animated Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn restless_reef_animation_is_native_and_lowers_to_independent_primitives() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/restless-reef-animation.json"
    ))
    .expect("Restless Reef animation ground truth is valid JSON");
    assert_eq!(truth["context"]["oracleText"], EXACT_ORACLE);

    let parsed = parse_animation(EXACT_ORACLE);
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
    assert_eq!(
        ability.rule.as_ref(),
        Some(&truth["abilities"][0]["expectedRule"])
    );
}

#[test]
fn other_cost_stats_colors_and_keyword_lists_reuse_the_same_children() {
    let parsed = parse_animation(
        "{3}{G}: Until end of turn, this land becomes a 2/3 green Shark creature with trample and vigilance. It's still a land.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical");
    assert_eq!(
        ability.rule.as_ref().unwrap(),
        &json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{3}{G}" }],
            "effects": [
                {
                    "kind": "becomeCreature",
                    "object": { "kind": "self" },
                    "addTypes": ["Creature"],
                    "addSubtypes": ["Shark"],
                    "addColors": ["G"],
                    "basePower": { "kind": "integer", "value": 2 },
                    "baseToughness": { "kind": "integer", "value": 3 },
                    "retainExistingTypes": true,
                    "duration": { "kind": "untilEndOfCurrentTurn" }
                },
                {
                    "kind": "grantKeyword",
                    "object": { "kind": "self" },
                    "keyword": "trample",
                    "duration": { "kind": "untilEndOfCurrentTurn" }
                },
                {
                    "kind": "grantKeyword",
                    "object": { "kind": "self" },
                    "keyword": "vigilance",
                    "duration": { "kind": "untilEndOfCurrentTurn" }
                }
            ]
        })
    );
}

#[test]
fn retention_type_must_be_distinct_from_the_new_animation_types() {
    let parsed = parse_animation(
        "{2}{U}{B}: Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a creature.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("temporary-source-animation-activation")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.temporary-animation.retained-type")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("creature"));
}
