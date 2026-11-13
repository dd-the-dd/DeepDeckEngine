use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse(oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Reanimate".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn exact_reanimate_is_native_and_lowers_to_three_independent_effects() {
    let parsed = parse(
        "Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native Reanimate rule");
    assert_eq!(rule["kind"], "spellAbility");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["zone"],
        json!({ "kind": "anyGraveyard" })
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
    let effects = rule["effects"].as_array().expect("primitive effects");
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0]["kind"], "bind");
    assert_eq!(effects[0]["value"]["kind"], "manaValueOf");
    assert_eq!(effects[1]["kind"], "moveCards");
    assert_eq!(effects[1]["to"]["kind"], "battlefield");
    assert_eq!(effects[2]["kind"], "loseLife");
    assert_eq!(effects[2]["amount"]["kind"], "boundValue");
    let serialized = rule.to_string();
    assert!(!serialized.contains("reanimateTargetCard"));
    assert!(!serialized.contains("resolveSpellInstruction"));
}

#[test]
fn claimed_wrong_destination_fails_natively_at_the_local_relation() {
    let parsed = parse(
        "Put target creature card from a graveyard into your hand. You lose life equal to that card's mana value.",
    );

    assert_eq!(parsed.status, "unsupported", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("graveyard-card-entry-life")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.graveyard-card-entry.destination-relation")
    );
}
