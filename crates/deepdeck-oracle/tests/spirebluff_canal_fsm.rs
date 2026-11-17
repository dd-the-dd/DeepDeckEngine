use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const ENTRY: &str = "This land enters tapped unless you control two or fewer other lands.";

fn parse_land(name: &str, mana_ability: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(format!("{ENTRY}\n{mana_ability}")),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn expected_entry_rule() -> Value {
    json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "condition": {
            "kind": "not",
            "operand": {
                "kind": "compare",
                "operator": "<=",
                "left": {
                    "kind": "countPermanents",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "where": { "kind": "cardTypeContains", "value": "Land" },
                },
                "right": { "kind": "integer", "value": 2 },
            },
        },
        "replacement": [{
            "kind": "setEnteringState",
            "object": { "kind": "self" },
            "tapped": true,
        }],
    })
}

#[test]
fn spirebluff_canal_is_fully_native_and_primitive() {
    let parsed = parse_land("Spirebluff Canal", "{T}: Add {U} or {R}.");

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&expected_entry_rule())
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": { "kind": "chooseOne", "options": ["{U}", "{R}"] },
            }],
        }))
    );
}

#[test]
fn the_same_entry_machine_covers_other_fast_land_color_pairs() {
    for (name, mana_ability, expected_options) in [
        ("Blackcleave Cliffs", "{T}: Add {B} or {R}.", ["{B}", "{R}"]),
        ("Seachrome Coast", "{T}: Add {W} or {U}.", ["{W}", "{U}"]),
    ] {
        let parsed = parse_land(name, mana_ability);
        assert_eq!(parsed.status, "canonical", "{name}: {parsed:#?}");
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.recognizer == RecognizerProvenance::Native),
            "{name}: {parsed:#?}"
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref(),
            Some(&expected_entry_rule())
        );
        assert_eq!(
            parsed.abilities[1].rule.as_ref().expect("mana rule")["effects"][0]["mana"]["options"],
            json!(expected_options)
        );
    }
}

fn assert_native_failure(text: &str, expected_state: &str, expected_found: &str) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Fast Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "unsupported", "{text}: {parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let diagnostic = ability.diagnostics.first().expect("structured failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("controlled-count-entry")
    );
    assert_eq!(diagnostic.state.as_deref(), Some(expected_state));
    assert_eq!(diagnostic.found.as_deref(), Some(expected_found));
}

#[test]
fn promoted_family_rejects_missing_other_unknown_comparator_and_trailing_tokens() {
    assert_native_failure(
        "This land enters tapped unless you control two or fewer lands.",
        "condition.controlled-permanent-count.complete",
        ".",
    );
    assert_native_failure(
        "This land enters tapped unless you control two or exactly other lands.",
        "condition.controlled-permanent-count.comparator-direction",
        "exactly",
    );
    assert_native_failure(
        "This land enters tapped unless you control two or fewer other lands. instead",
        "replacement.controlled-count-entry.complete",
        "instead",
    );
}
