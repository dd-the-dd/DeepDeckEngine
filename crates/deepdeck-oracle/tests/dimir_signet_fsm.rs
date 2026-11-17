use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const DIMIR_SIGNET_ORACLE: &str = "{1}, {T}: Add {U}{B}.";

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
fn mana_payment_and_tap_compose_into_one_native_mana_ability() {
    let parsed = parse(DIMIR_SIGNET_ORACLE);
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
                "mana": "{U}{B}",
            }],
        }))
    );
}

#[test]
fn compound_payment_and_fixed_output_sequences_are_generic() {
    let parsed = parse("{2}{W}, {T}: Add {R}{G}{C}.");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("sibling emits primitive IR");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["costs"][0]["manaCost"], "{2}{W}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["effects"][0]["mana"], "{R}{G}{C}");
}

#[test]
fn unsupported_fixed_output_symbol_fails_in_the_shared_mana_child() {
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
fn dangling_compound_cost_is_a_committed_native_failure() {
    let parsed = parse("{1}, {T}, : Add {U}{B}.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("activation.compound.cost")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some(":"));
}

#[test]
fn trailing_unconsumed_text_is_reported_by_the_native_envelope() {
    let parsed = parse("{1}, {T}: Add {U}{B}. Abracadabra.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Abracadabra"));
}

#[test]
fn dimir_signet_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(DIMIR_SIGNET_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
