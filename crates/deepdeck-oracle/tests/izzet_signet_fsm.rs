use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const IZZET_SIGNET_ORACLE: &str = "{1}, {T}: Add {U}{R}.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Mana Artifact".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn izzet_signet_is_native_primitive_mana_ir() {
    let parsed = parse(IZZET_SIGNET_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "payMana", "manaCost": "{1}" },
                { "kind": "tap", "object": { "kind": "self" } },
            ],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": "{U}{R}",
            }],
        }))
    );
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
}

#[test]
fn sibling_compound_cost_and_fixed_output_use_the_same_automata() {
    let parsed = parse("{2}{G}, {T}: Add {W}{B}{C}.");
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("sibling emits primitive IR");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(rule["costs"][0]["manaCost"], "{2}{G}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["effects"][0]["mana"], "{W}{B}{C}");
}

#[test]
fn unsupported_output_symbol_is_a_committed_shared_child_failure() {
    let parsed = parse("{1}, {T}: Add {U}{X}.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("mana.fixed.symbol")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("{X}"));
}

#[test]
fn dangling_compound_cost_does_not_fall_back() {
    let parsed = parse("{1}, {T}, : Add {U}{R}.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("cost.payment.verb")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some(":"));
}
