use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_mana_ability(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Mana Source".to_string(),
        type_line: "Creature - Elf Druid".to_string(),
        mana_cost: Some("{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn tap_for_fixed_mana_is_native_and_uses_only_primitive_ir() {
    for (oracle_text, expected_mana) in [
        ("{T}: Add {G}.", "{G}"),
        ("{T}: Add {C}{C}.", "{C}{C}"),
        ("{T}: Add {W}{U}.", "{W}{U}"),
    ] {
        let parsed = parse_mana_ability(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref(),
            Some(&json!({
                "kind": "manaAbility",
                "source": { "kind": "self" },
                "costs": [{
                    "kind": "tap",
                    "object": { "kind": "self" },
                }],
                "effects": [{
                    "kind": "addMana",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "mana": expected_mana,
                }],
            }))
        );
    }
}

#[test]
fn unsupported_braced_output_is_a_committed_native_failure() {
    let parsed = parse_mana_ability("{T}: Add {Q}.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert_eq!(diagnostic.state.as_deref(), Some("mana.fixed.start"));
    assert_eq!(diagnostic.found.as_deref(), Some("{Q}"));
}

#[test]
fn open_ended_mana_abilities_remain_outside_the_compact_boundary() {
    for oracle_text in [
        "{T}: Add one mana of that color.",
        "{T}: Add {G}. Spend this mana only to cast creature spells.",
    ] {
        let parsed = parse_mana_ability(oracle_text);
        assert_ne!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
    }
}

#[test]
fn compound_costs_and_unrestricted_color_choice_compose_natively() {
    let parsed = parse_mana_ability("{T}, Pay 2 life: Add two mana of any color.");
    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                {
                    "kind": "payLife",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "amount": { "kind": "integer", "value": 2 },
                },
            ],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": { "kind": "chooseColor", "amount": 2 },
            }],
        }))
    );
}

#[test]
fn claimed_compound_color_mana_near_misses_fail_locally() {
    for (oracle_text, state, found) in [
        (
            "{T}, Pay X life: Add one mana of any color.",
            "cost.pay-life.amount-supported",
            "X",
        ),
        (
            "{T}, Pay 1 life: Add one mana of any hue.",
            "effect.add-any-color-mana.color",
            "hue",
        ),
    ] {
        let parsed = parse_mana_ability(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("activated-mana-ability")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
