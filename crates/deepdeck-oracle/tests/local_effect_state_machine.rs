use deepdeck_oracle::oracle::{OracleCardParseRequest, parse_oracle_card};

fn parse(name: &str, oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn exact_damage_then_life_wording_lowers_to_two_primitives() {
    let parsed = parse(
        "Lightning Helix",
        "Lightning Helix deals 3 damage to any target and you gain 3 life.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("canonical rule");
    let kinds = rule["effects"]
        .as_array()
        .expect("ordered effects")
        .iter()
        .map(|effect| effect["kind"].as_str().expect("primitive kind"))
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["dealDamage", "gainLife"]);
    assert_eq!(rule["effects"][0]["amount"]["value"], 3);
    assert_eq!(rule["effects"][1]["amount"]["value"], 3);
    assert_eq!(
        rule["declaration"]["decisions"][0]["id"],
        "targetDamageable"
    );
    assert!(
        ability
            .iterations
            .iter()
            .any(|iteration| iteration.result_text.contains("dealDamage"))
    );
}

#[test]
fn source_pronoun_variable_and_player_target_reuse_the_same_local_automata() {
    let parsed = parse(
        "Example Source",
        "This spell deals X damage to target player then you gain three life.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.as_ref().expect("canonical rule");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "xValue");
    assert_eq!(rule["declaration"]["decisions"][1]["id"], "targetPlayer");
    assert_eq!(rule["effects"][0]["amount"]["decisionId"], "xValue");
    assert_eq!(rule["effects"][1]["amount"]["value"], 3);
}

#[test]
fn claimed_family_with_missing_amount_cannot_fall_back_to_a_legacy_leaf() {
    let parsed = parse(
        "Example Source",
        "Example Source deals damage to any target and you gain 3 life.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    // `deals damage to <target> equal to <amount>` is a supported postposed
    // amount, so the missing amount is detected after the target list.
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.damage.postposed-amount")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("and"));
}

#[test]
fn unlinked_unknown_source_is_not_guessed_by_a_legacy_leaf() {
    let parsed = parse(
        "Different Source",
        "Unknown Source deals 3 damage to any target and you gain 3 life.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("reference.source.unresolved-name")
    );
    assert_eq!(diagnostic.token_index, Some(0));
    assert_eq!(diagnostic.span_start, Some(0));
    // `Source` is ordinary Oracle vocabulary and does not align with the
    // current-card alias (`Different Source`). Keep the diagnostic on the
    // unresolved lexical core instead of guessing a wider card name.
    assert_eq!(diagnostic.span_end, Some(7));
    assert_eq!(diagnostic.found.as_deref(), Some("Unknown"));
}

#[test]
fn a_connector_inside_a_trigger_envelope_is_not_claimed_as_a_spell_effect_sequence() {
    let parsed = parse(
        "Example Source",
        "Whenever Example Source deals damage and you gain 3 life, draw a card.",
    );

    // The trigger frame claims the ability: the dealt-damage event is read,
    // and the conjoined second event is rejected at the envelope separator;
    // no spell effect sequence claims the `and`.
    assert_eq!(parsed.status, "unsupported");
    let ability = &parsed.abilities[0];
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("generic-trigger-program")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("trigger.envelope.separator")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("and"));
}

#[test]
fn a_connector_inside_a_condition_is_not_claimed_as_top_level_effect_sequencing() {
    let parsed = parse(
        "Example Source",
        "If this spell deals damage and you gain 3 life, draw a card.",
    );

    // The shared program enters the conditional head and rejects its
    // unsupported verb before any connector inside it could be read as
    // top-level sequencing.
    assert_eq!(parsed.status, "unsupported");
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("condition.sentence.object.verb")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("deals"));
    assert_eq!(diagnostic.token_index, Some(3));
}
