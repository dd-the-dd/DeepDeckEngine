use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Flash\nWhen this creature enters and whenever an opponent draws a card except the first one they draw in each of their draw steps, this creature deals 1 damage to any target. Then amass Orcs 1.";

fn parse_card(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Local Trigger".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn orcish_bowmasters_is_native_event_union_and_atomic_effect_sequence() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/orcish-bowmasters.json"
    ))
    .expect("Orcish Bowmasters ground truth is valid JSON");
    assert_eq!(truth["context"]["oracleText"], EXACT_ORACLE);

    let parsed = parse_card("Creature — Orc Archer", EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(
        parsed
            .abilities
            .iter()
            .flat_map(|ability| ability.entities.iter())
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&truth["abilities"][0]["expectedRule"])
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&truth["abilities"][1]["expectedRule"])
    );
}

#[test]
fn another_source_type_target_selector_amass_subtype_and_counts_reuse_the_children() {
    let parsed = parse_card(
        "Artifact",
        "When this artifact enters and whenever an opponent draws a card except the first one they draw in each of their draw steps, this artifact deals 2 damage to target creature. Then amass Zombies 2.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical");
    assert_eq!(
        ability.rule.as_ref().unwrap()["declaration"]["decisions"][0],
        json!({
            "kind": "chooseTargets",
            "id": "triggerDamageTarget",
            "minimum": 1,
            "maximum": 1,
            "candidates": {
                "kind": "permanents",
                "where": { "kind": "cardTypeContains", "value": "Creature" }
            }
        })
    );
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"],
        json!([
            {
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": 2 },
                "recipient": { "kind": "chosenTarget", "id": "triggerDamageTarget" }
            },
            {
                "kind": "amass",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "armySubtype": "Zombie",
                "count": { "kind": "integer", "value": 2 }
            }
        ])
    );
}

#[test]
fn second_draw_exception_is_a_committed_event_diagnostic() {
    let parsed = parse_card(
        "Creature — Orc Archer",
        "When this creature enters and whenever an opponent draws a card except the second one they draw in each of their draw steps, this creature deals 1 damage to any target. Then amass Orcs 1.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("entry-or-repeat-trigger")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("event.card-drawn.exception.ordinal")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("second"));
}
