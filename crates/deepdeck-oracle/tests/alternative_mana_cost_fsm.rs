use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ALTERNATIVE_COST: &str = "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana cost.";

fn parse_card(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{3}{U}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn alternative_cost_rule(amount: i64, color: &str) -> serde_json::Value {
    json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "alternativeCost",
            "costs": [
                {
                    "kind": "payLife",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "amount": { "kind": "integer", "value": amount },
                },
                {
                    "kind": "exileCard",
                    "zone": {
                        "kind": "hand",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                    },
                    "where": { "kind": "colorContains", "value": color },
                },
            ],
        },
    })
}

#[test]
fn compound_alternative_cost_and_counter_are_both_native_primitive_abilities() {
    let parsed = parse_card(
        "Force of Will",
        &format!("{ALTERNATIVE_COST}\nCounter target spell."),
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&alternative_cost_rule(1, "blue"))
    );
    let counter = parsed.abilities[1].rule.as_ref().expect("counter rule");
    assert_eq!(counter["effects"][0]["kind"], "counterSpell");
    assert_eq!(counter["effects"][0]["spell"]["id"], "targetSpell");
}

#[test]
fn alternative_cost_vocabulary_is_typed_before_the_automata_run() {
    let parsed = parse_card("Lexical Alternative", ALTERNATIVE_COST);
    let entities = &parsed.abilities[0].entities;
    let token_type = |raw: &str| {
        entities
            .iter()
            .find(|entity| entity.raw.eq_ignore_ascii_case(raw))
            .map(|entity| entity.token_type.as_str())
    };

    assert_eq!(token_type("may"), Some("modalityWord"));
    assert_eq!(token_type("pay"), Some("costActionWord"));
    assert_eq!(token_type("blue"), Some("colorWord"));
    assert_eq!(token_type("hand"), Some("zoneWord"));
    assert_eq!(token_type("rather"), Some("substitutionWord"));
    assert_eq!(token_type("than"), Some("substitutionWord"));
    assert_eq!(token_type("spell's"), Some("possessiveTypeWord"));
    assert_eq!(token_type("mana"), Some("costWord"));
    assert_eq!(token_type("cost"), Some("costWord"));
    assert!(entities.iter().all(|entity| entity.token_type != "unknown"));
}

#[test]
fn sibling_values_reuse_the_same_cost_automata() {
    let parsed = parse_card(
        "Sibling Alternative",
        "You may pay 2 life and exile a red card from your hand rather than pay this spell's mana cost.",
    );

    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&alternative_cost_rule(2, "red"))
    );
}

#[test]
fn malformed_family_members_fail_at_the_responsible_local_state() {
    for (oracle_text, expected_state, expected_found) in [
        (
            "You may pay 1 life and exile a purple card from your hand rather than pay this spell's mana cost.",
            "cost.exile-card.selector.color",
            "purple",
        ),
        (
            "You may pay 1 life and exile a blue card from your library rather than pay this spell's mana cost.",
            "cost.exile-card.zone",
            "library",
        ),
        (
            "You may pay 1 life or exile a blue card from your hand rather than pay this spell's mana cost.",
            "cost.sequence.connector",
            "or",
        ),
        (
            "You may pay X life and exile a blue card from your hand rather than pay this spell's mana cost.",
            "cost.pay-life.amount-supported",
            "X",
        ),
    ] {
        let parsed = parse_card("Malformed Alternative", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("alternative-mana-cost"),
            "{oracle_text}"
        );
        assert_eq!(
            diagnostic.state.as_deref(),
            Some(expected_state),
            "{oracle_text}"
        );
        assert_eq!(
            diagnostic.found.as_deref(),
            Some(expected_found),
            "{oracle_text}"
        );
    }
}

#[test]
fn distinct_alternative_cost_envelopes_remain_outside_this_promoted_boundary() {
    for oracle_text in ["You may pay {1} rather than pay this spell's mana cost."] {
        // The shared program rejects the other envelope explicitly.
        let parsed = parse_card("Other Alternative", oracle_text);
        assert_eq!(parsed.status, "unsupported", "{oracle_text}");
        let ability = &parsed.abilities[0];
        assert!(ability.rule.is_none(), "{oracle_text}");
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some("contextual-effect-program"),
            "{oracle_text}"
        );
    }
}
