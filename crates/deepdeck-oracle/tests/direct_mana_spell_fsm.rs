use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_instant(card_name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn fixed_mana_spell_rule(mana: &str) -> serde_json::Value {
    json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "addMana",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "mana": mana,
        }],
    })
}

#[test]
fn dark_ritual_is_native_and_matches_its_exact_ground_truth_fixture() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/dark-ritual.json"
    ))
    .expect("Dark Ritual ground truth is valid JSON");
    let context = &truth["context"];
    let parsed = parse_instant(
        context["name"].as_str().expect("fixture card name"),
        context["manaCost"].as_str().expect("fixture mana cost"),
        context["oracleText"].as_str().expect("fixture Oracle text"),
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(
        parsed.abilities[0]
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule,
        Some(truth["abilities"][0]["expectedRule"].clone())
    );
}

#[test]
fn fixed_mana_spell_sibling_reuses_the_same_envelope_and_atomic_effect() {
    let parsed = parse_instant("Seething Song", "{2}{R}", "Add {R}{R}{R}{R}{R}.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(fixed_mana_spell_rule("{R}{R}{R}{R}{R}"))
    );
}

#[test]
fn unsupported_suffix_is_a_committed_direct_mana_spell_failure() {
    let parsed = parse_instant(
        "Conditional Ritual",
        "{B}",
        "Add {B}{B}{B} unless an opponent pays {1}.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("contextual-effect-program")
    );
    // The shared sequence owns an `unless` frame. `an opponent` opens no
    // payer the frame binds, so the clause is read by the one condition
    // language, which has no atom for it.
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("condition.unless.atom-kind")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("an"));
}
