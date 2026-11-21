use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str =
    "{T}: Add one mana of any color that a land an opponent controls could produce.";

fn parse(oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Fellwar Stone".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn expected_rule(amount: i64, costs: serde_json::Value) -> serde_json::Value {
    json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": costs,
        "effects": [{
            "kind": "addMana",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "mana": {
                "kind": "manaColorsLandsOpponentsControlCouldProduce",
                "amount": amount,
            },
        }],
    })
}

#[test]
fn exact_card_is_native_and_preserves_the_existing_primitive_ir() {
    let parsed = parse(EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&expected_rule(
            1,
            json!([{ "kind": "tap", "object": { "kind": "self" } }])
        ))
    );
    assert!(
        ability
            .entities
            .iter()
            .all(|token| token.token_type != "unknown")
    );
}

#[test]
fn amount_and_compound_cost_siblings_share_the_same_children() {
    let parsed = parse(
        "{1}, {T}: Add two mana of any color that a land an opponent controls could produce.",
    );
    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&expected_rule(
            2,
            json!([
                { "kind": "payMana", "manaCost": "{1}" },
                { "kind": "tap", "object": { "kind": "self" } },
            ])
        ))
    );
}

#[test]
fn claimed_near_misses_report_the_responsible_local_state() {
    for (oracle, state) in [
        (
            "{T}: Add one mana of any color that an artifact an opponent controls could produce.",
            "effect.add-land-color-domain.land-selector.validation",
        ),
        (
            "{T}: Add one mana of any color that a land you control could produce.",
            "effect.add-land-color-domain.controller-scope",
        ),
        (
            "{T}: Add one mana of any color that a land an opponent controls can produce.",
            "effect.add-land-color-domain.capability",
        ),
        (
            "{T}: Add one mana of any color that a land an opponent controls could produce twice.",
            "activation.mana.terminator",
        ),
    ] {
        let parsed = parse(oracle);
        assert_eq!(parsed.status, "unsupported", "{oracle}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("activated-mana-ability")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle}");
    }
}

#[test]
fn unrestricted_and_commander_color_domains_keep_their_existing_routes() {
    for oracle in [
        "{T}: Add one mana of any color.",
        "{T}: Add one mana of any color in your commander's color identity.",
    ] {
        let parsed = parse(oracle);
        assert_eq!(parsed.status, "canonical", "{oracle}: {parsed:#?}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle}"
        );
    }
}
