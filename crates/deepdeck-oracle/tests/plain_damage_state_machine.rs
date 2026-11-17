use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse(
    name: &str,
    mana_cost: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn lightning_bolt_is_one_entirely_native_primitive_damage_ability() {
    let parsed = parse(
        "Lightning Bolt",
        "{R}",
        "Lightning Bolt deals 3 damage to any target.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("canonical primitive rule");
    assert_eq!(rule["kind"], "spellAbility");
    assert_eq!(
        rule["declaration"]["decisions"][0]["id"],
        "targetDamageable"
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"],
        json!({ "kind": "anyTarget" })
    );
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][0]["amount"]["value"], 3);
    assert_eq!(
        rule["effects"][0]["recipient"],
        json!({ "kind": "chosenTarget", "id": "targetDamageable" })
    );
}

#[test]
fn variable_amount_and_union_selector_reuse_numeric_and_target_automata() {
    let parsed = parse(
        "Test Spark",
        "{X}{R}",
        "Test Spark deals X damage to target creature or planeswalker.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("canonical rule");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "xValue");
    assert_eq!(rule["declaration"]["decisions"][1]["id"], "targetPermanent");
    assert_eq!(rule["effects"][0]["amount"]["kind"], "decisionResult");
    assert_eq!(rule["effects"][0]["amount"]["decisionId"], "xValue");
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"]["where"]["kind"],
        "or"
    );
}

#[test]
fn committed_plain_damage_with_missing_amount_fails_in_numeric_atom() {
    let parsed = parse(
        "Test Spark",
        "{R}",
        "Test Spark deals damage to any target.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("plain-damage-effect"));
    assert_eq!(diagnostic.state.as_deref(), Some("numeric.atom.start"));
    assert_eq!(diagnostic.found.as_deref(), Some("damage"));
}

#[test]
fn unresolved_source_name_is_not_guessed_or_sent_to_legacy() {
    let parsed = parse(
        "Test Spark",
        "{R}",
        "Other Spark deals 2 damage to any target.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("reference.source.unresolved-name")
    );
    assert_eq!(diagnostic.span_start, Some(0));
    assert_eq!(diagnostic.span_end, Some("Other Spark".len()));
}

#[test]
fn unknown_target_kind_is_a_committed_target_list_failure() {
    let parsed = parse(
        "Test Spark",
        "{R}",
        // `Hero` is now a cataloged creature subtype; use an unknown noun.
        "Test Spark deals 2 damage to target widget.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("plain-damage-effect"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("selector.predicate.start")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("widget"));
}

#[test]
fn unowned_sequence_connector_is_not_falsely_claimed_as_plain_damage() {
    let parsed = parse(
        "Test Spark",
        "{R}",
        "Test Spark deals 2 damage to any target then.",
    );

    // The shared sequence owns the dangling connector; plain damage never
    // claims the ability and nothing is accepted.
    assert_eq!(parsed.status, "unsupported");
    let ability = &parsed.abilities[0];
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_ne!(diagnostic.automaton.as_deref(), Some("plain-damage-effect"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("sequence.connector.effect")
    );
}
