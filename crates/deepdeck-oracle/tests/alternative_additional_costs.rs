use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn discard_or_life_additional_cost_composes_existing_cost_primitives() {
    let parsed = parse_instant(
        "As an additional cost to cast this spell, discard a card or pay 3 life.\nDestroy target creature or planeswalker.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let cost_rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the alternative additional cost should parse");
    let declaration = &cost_rule["declaration"];
    assert_eq!(declaration["decisions"][0]["id"], "additionalCostMode");
    assert_eq!(
        declaration["decisions"][0]["options"],
        serde_json::json!(["discard", "payLife"])
    );
    assert_eq!(declaration["decisions"][1]["condition"]["value"], "discard");
    assert_eq!(
        declaration["additionalCosts"][0]["then"][0]["kind"],
        "discardCard"
    );
    assert_eq!(
        declaration["additionalCosts"][1]["then"][0]["kind"],
        "payLife"
    );
    assert_eq!(
        declaration["additionalCosts"][1]["then"][0]["amount"]["value"],
        3
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().expect("destroy effect")["effects"][0]["kind"],
        "destroyPermanent"
    );
}

#[test]
fn option_order_and_life_amount_are_independent_of_the_envelope() {
    let parsed = parse_instant(
        "As an additional cost to cast this spell, pay 5 life or discard one card.\nDestroy target artifact.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let declaration = &parsed.abilities[0].rule.as_ref().unwrap()["declaration"];
    assert_eq!(
        declaration["decisions"][0]["options"],
        serde_json::json!(["payLife", "discard"])
    );
    assert_eq!(
        declaration["additionalCosts"][0]["then"][0]["amount"]["value"],
        5
    );
    assert_eq!(
        declaration["additionalCosts"][1]["then"][0]["kind"],
        "discardCard"
    );
}

#[test]
fn alternative_additional_costs_reuse_other_shared_cost_atoms() {
    let parsed = parse_instant(
        "As an additional cost to cast this spell, sacrifice a creature or pay 4 life.\nDestroy target artifact.",
    );

    assert_eq!(parsed.status, "canonical");
    let declaration = &parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the sibling cost should parse")["declaration"];
    assert_eq!(
        declaration["decisions"][0]["options"],
        serde_json::json!(["sacrifice", "payLife"])
    );
    assert_eq!(
        declaration["additionalCosts"][0]["then"][0]["kind"],
        "sacrificePermanent"
    );
    assert_eq!(
        declaration["additionalCosts"][1]["then"][0]["kind"],
        "payLife"
    );

    let nested_selector = parse_instant(
        "As an additional cost to cast this spell, sacrifice an artifact or creature or pay {4}.\nDestroy target artifact.",
    );
    assert_eq!(nested_selector.status, "canonical");
    assert_eq!(
        nested_selector.abilities[0]
            .rule
            .as_ref()
            .expect("an or inside a selector is not the cost boundary")["declaration"]["decisions"]
            [0]["options"],
        serde_json::json!(["sacrifice", "payMana"])
    );
}

#[test]
fn unsupported_alternative_is_not_silently_accepted_as_a_cost() {
    let parsed = parse_instant(
        "As an additional cost to cast this spell, discard a card or lose 3 life.\nDestroy target creature.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn claimed_malformed_members_fail_at_the_responsible_cost_state() {
    for (oracle_text, state, found) in [
        (
            "As an additional cost to cast this spell, discard a card or pay X life.\nDestroy target creature.",
            "cost.pay-life.amount-supported",
            "X",
        ),
        (
            "As an additional cost to cast this spell, discard a card or pay 3 life immediately.\nDestroy target creature.",
            "additional-cost.alternative.connector-or-terminator",
            "immediately",
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        let diagnostic = ability.diagnostics.first().unwrap();
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("alternative-additional-cost")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
