use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const PYROBLAST_ORACLE: &str =
    "Choose one —\n• Counter target spell if it's blue.\n• Destroy target permanent if it's blue.";

fn parse_modal(card_name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn pyroblast_is_a_native_color_guarded_modal_spell() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/pyroblast.json"
    ))
    .expect("Pyroblast ground-truth fixture is valid JSON");
    let context = &truth["context"];
    assert_eq!(context["oracleText"], PYROBLAST_ORACLE);

    let parsed = parse_modal(
        context["name"].as_str().expect("fixture card name"),
        context["manaCost"].as_str().expect("fixture mana cost"),
        PYROBLAST_ORACLE,
    );

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
fn opposite_color_sibling_reuses_the_same_modes_targets_and_resolution_guards() {
    let parsed = parse_modal(
        "Test Modal Blast",
        "{U}",
        "Choose one —\n• Counter target spell if it's red.\n• Destroy target permanent if it's red.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"],
        json!({ "kind": "spells" })
    );
    assert_eq!(
        rule["declaration"]["decisions"][2]["candidates"],
        json!({ "kind": "permanents", "where": null })
    );
    assert_eq!(
        rule["effects"][0]["then"][0]["condition"]["where"],
        json!({ "kind": "colorContains", "value": "Red" })
    );
    assert_eq!(
        rule["effects"][1]["then"][0]["condition"]["where"],
        json!({ "kind": "colorContains", "value": "Red" })
    );
}

#[test]
fn claimed_near_misses_fail_at_the_responsible_local_state() {
    for (oracle_text, state, found) in [
        (
            "Choose one —\n• Counter target spell if it's blue.\n• Exile target permanent if it's blue.",
            "modal.mode.action",
            "Exile",
        ),
        (
            "Choose one —\n• Counter target spell if it's blue.\n• Destroy target card if it's blue.",
            "effect.zone-change.target.validation",
            "if",
        ),
        (
            "Choose one —\n• Counter target spell if it's blue.\n• Destroy target permanent if it's colorless.",
            "condition.target-color.color",
            ".",
        ),
    ] {
        let parsed = parse_modal("Test Modal Near Miss", "{R}", oracle_text);
        let ability = &parsed.abilities[0];

        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert!(ability.rule.is_none());
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some("color-guarded-choose-one-modal")
        );
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
}
