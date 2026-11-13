use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};

fn parse(counter_name: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Context Sage".to_string(),
        type_line: "Creature — Human Wizard".to_string(),
        mana_cost: Some("{X}{U}".to_string()),
        oracle_text: Some(format!(
            "When this creature enters, put X {counter_name} counters on it. Then draw half X cards, rounded down."
        )),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn open_counter_names_share_the_native_put_counters_path() {
    for (counter_name, expected) in [
        ("+1/+1", "+1/+1"),
        ("flying", "flying"),
        ("charge", "charge"),
        ("oil", "oil"),
        ("flux capacitor calibration", "flux capacitor calibration"),
        ("Flux-Capacitor Calibration", "flux-capacitor calibration"),
    ] {
        let parsed = parse(counter_name);
        assert_eq!(parsed.status, "canonical", "{counter_name}: {parsed:#?}");
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert!(
            ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown"),
            "{counter_name}: {:#?}",
            ability.entities
        );
        assert_eq!(
            ability.rule.as_ref().unwrap()["effects"][0]["counter"],
            expected
        );
    }
}

#[test]
fn malformed_committed_counter_phrase_reports_the_local_relation_state() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Context Sage".to_string(),
        type_line: "Creature — Human Wizard".to_string(),
        mana_cost: Some("{X}{U}".to_string()),
        oracle_text: Some(
            "When this creature enters, put X charge counters beside it. Then draw half X cards, rounded down."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "unsupported");
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.put-counters.relation")
    );
    assert_eq!(diagnostic.expected.as_deref(), Some("on"));
}

#[test]
fn a_non_counter_put_effect_is_not_claimed_by_the_counter_family() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Context Sage".to_string(),
        type_line: "Creature — Human Wizard".to_string(),
        mana_cost: Some("{X}{U}".to_string()),
        oracle_text: Some(
            "When this creature enters, put X cards from your graveyard into your hand. Then draw half X cards, rounded down."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "unsupported");
    assert!(
        parsed.abilities[0]
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.automaton.as_deref() != Some("effect.put-counters"))
    );
}
