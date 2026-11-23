use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const HALLOWED_FOUNTAIN_ORACLE: &str = "({T}: Add {W} or {U}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.";

fn parse_land(
    card_name: &str,
    type_line: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn assert_native_without_unknowns(parsed: &deepdeck_oracle::oracle::OracleCardParseResult) {
    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(parsed.abilities.iter().all(|ability| {
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    }));
}

fn expected_mana_rule(options: [&str; 2]) -> serde_json::Value {
    json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "addMana",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "mana": { "kind": "chooseOne", "options": options },
        }],
    })
}

fn expected_optional_life_entry_rule() -> serde_json::Value {
    json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "decisions": [{
            "id": "optionalLifePayment",
            "kind": "chooseWhetherToPay",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "cost": {
                "kind": "payLife",
                "amount": { "kind": "integer", "value": 2 },
            },
        }],
        "replacement": [{
            "kind": "conditional",
            "condition": {
                "kind": "not",
                "operand": { "kind": "costWasPaid", "decisionId": "optionalLifePayment" },
            },
            "then": [{
                "kind": "setEnteringState",
                "object": { "kind": "self" },
                "tapped": true,
            }],
        }],
    })
}

#[test]
fn hallowed_fountain_is_fully_native_and_lowers_to_primitive_rules() {
    let parsed = parse_land(
        "Hallowed Fountain",
        "Land — Plains Island",
        HALLOWED_FOUNTAIN_ORACLE,
    );

    assert_native_without_unknowns(&parsed);
    assert_eq!(parsed.abilities.len(), 2);
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&expected_mana_rule(["{W}", "{U}"]))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&expected_optional_life_entry_rule())
    );
}

#[test]
fn sibling_shocklands_reuse_the_same_mana_and_entry_automata() {
    for (name, type_line, oracle, options) in [
        (
            "Sacred Foundry",
            "Land — Mountain Plains",
            "({T}: Add {R} or {W}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.",
            ["{R}", "{W}"],
        ),
        (
            "Steam Vents",
            "Land — Island Mountain",
            "({T}: Add {U} or {R}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.",
            ["{U}", "{R}"],
        ),
    ] {
        let parsed = parse_land(name, type_line, oracle);
        assert_native_without_unknowns(&parsed);
        assert_eq!(
            parsed.abilities[0].rule.as_ref(),
            Some(&expected_mana_rule(options)),
            "{name}"
        );
        assert_eq!(
            parsed.abilities[1].rule.as_ref(),
            Some(&expected_optional_life_entry_rule()),
            "{name}"
        );
    }
}

#[test]
fn shockland_family_commits_a_structured_failure_for_the_wrong_payment_subject() {
    let parsed = parse_land(
        "Test Shockland",
        "Land",
        "As this land enters, you may pay 2 life. If they don't, it enters tapped.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("the promoted family must retain its structured failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("optional-life-entry"));
    // `If they don't` is read by the shared condition language, which has
    // no `they` performer for the paying player: it fails at the atom.
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("condition.envelope.atom-kind")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("they"));
}
