use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str =
    "Permanents you control gain hexproof and indestructible until end of turn.";

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Intervention".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn heroic_intervention_is_native_and_lowers_each_keyword_atom_independently() {
    let parsed = parse_spell(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown"),
        "the promoted Oracle vocabulary must be fully classified"
    );
    let rule = ability.rule.as_ref().expect("primitive keyword grant");
    let effects = rule["effects"].as_array().expect("keyword effects");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "grantKeyword");
    assert_eq!(effects[0]["keyword"], "hexproof");
    assert_eq!(effects[1]["kind"], "grantKeyword");
    assert_eq!(effects[1]["keyword"], "indestructible");
    for effect in effects {
        assert_eq!(
            effect["object"],
            json!({
                "kind": "eachPermanent",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "where": null,
            })
        );
        assert_eq!(effect["duration"]["kind"], "untilEndOfCurrentTurn");
    }
    assert!(!rule.to_string().contains("grantKeywords"));
    assert!(!rule.to_string().contains("\"value\":\"Permanent\""));
}

#[test]
fn controlled_sets_keyword_lists_and_duration_generalize_independently() {
    for (oracle, expected_keywords, expected_filter) in [
        (
            "Creatures you control gain flying, vigilance, and lifelink until end of turn.",
            vec!["flying", "vigilance", "lifelink"],
            json!({ "kind": "cardTypeContains", "value": "Creature" }),
        ),
        (
            "Artifacts and creatures you control gain hexproof until end of turn.",
            vec!["hexproof"],
            json!({
                "kind": "or",
                "operands": [
                    { "kind": "cardTypeContains", "value": "Artifact" },
                    { "kind": "cardTypeContains", "value": "Creature" },
                ],
            }),
        ),
        (
            "Nonland permanents you control also gain indestructible until end of turn.",
            vec!["indestructible"],
            json!({
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Land" },
            }),
        ),
    ] {
        let parsed = parse_spell(oracle);
        assert_eq!(parsed.status, "canonical", "{oracle}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle}"
        );
        let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
            .as_array()
            .unwrap();
        assert_eq!(
            effects
                .iter()
                .map(|effect| effect["keyword"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_keywords,
            "{oracle}"
        );
        assert_eq!(effects[0]["object"]["where"], expected_filter, "{oracle}");
    }
}

#[test]
fn malformed_members_fail_at_the_responsible_child_without_legacy_fallback() {
    for (oracle, expected_state, expected_found) in [
        (
            "Permanents you control gain hexproof and until end of turn.",
            "keyword-list.item",
            "until",
        ),
        (
            "Permanents you control gain ward until end of turn.",
            "keyword-list.item",
            "ward",
        ),
        (
            "Permanents you control gain hexproof until your next turn.",
            "duration.end",
            "your",
        ),
        (
            "Permanents you control gain hexproof until end of turn unexpectedly.",
            "effect.keyword-grant.terminator",
            "unexpectedly",
        ),
    ] {
        let parsed = parse_spell(oracle);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{oracle}");
        assert_eq!(ability.status, "unsupported", "{oracle}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("controlled-keyword-grant"),
            "{oracle}"
        );
        assert_eq!(
            diagnostic.state.as_deref(),
            Some(expected_state),
            "{oracle}"
        );
        assert_eq!(
            diagnostic.found.as_deref(),
            Some(expected_found),
            "{oracle}"
        );
    }
}
