use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Exile".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn exile_then_controller_life_snapshots_values_before_the_zone_change() {
    for oracle_text in [
        "Exile target creature. Its controller gains life equal to its power.",
        "Exile up to one target artifact creature. Its controller gains life equal to its power.",
    ] {
        let parsed = parse_spell(oracle_text);
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the spell should produce primitive IR");
        let effects = rule["effects"].as_array().expect("ordered effects");

        assert_eq!(effects.len(), 4);
        assert_eq!(effects[0]["kind"], "bind");
        assert_eq!(effects[0]["value"]["kind"], "controllerOf");
        assert_eq!(effects[1]["kind"], "bind");
        assert_eq!(effects[1]["value"]["kind"], "maximum");
        assert_eq!(effects[1]["value"]["operands"][0]["kind"], "powerOf");
        assert_eq!(effects[1]["value"]["operands"][1]["value"], 0);
        assert_eq!(effects[2]["kind"], "exilePermanent");
        assert_eq!(effects[3]["kind"], "gainLife");
        assert_eq!(
            effects[3]["player"]["id"], effects[0]["id"],
            "life recipient must use the pre-exile controller snapshot"
        );
        assert_eq!(
            effects[3]["amount"]["id"], effects[1]["id"],
            "life amount must use the pre-exile power snapshot"
        );
        assert!(
            !rule
                .to_string()
                .contains("exilePermanentAndControllerGainsPower")
        );
    }
}

#[test]
fn owner_or_toughness_followups_compose_without_approximating_controller_and_power() {
    // The shared player-action child reads `its owner` and the shared numeric
    // child reads `its toughness`; the runtime resolves both from the target's
    // last known information (CR 608.2h). Neither becomes controller/power.
    for (oracle_text, relation, property) in [
        (
            "Exile target creature. Its owner gains life equal to its power.",
            "ownerOf",
            "powerOf",
        ),
        (
            "Exile target creature. Its controller gains life equal to its toughness.",
            "controllerOf",
            "toughnessOf",
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the composed sequence should produce primitive IR");
        let effects = rule["effects"].as_array().expect("ordered effects");
        assert_eq!(effects.len(), 2, "{oracle_text}");
        assert_eq!(effects[0]["kind"], "exilePermanent");
        let target = &effects[0]["permanent"];
        assert_eq!(target["kind"], "chosenTarget");
        assert_eq!(effects[1]["kind"], "gainLife");
        assert_eq!(effects[1]["player"]["kind"], relation, "{oracle_text}");
        assert_eq!(&effects[1]["player"]["object"], target);
        assert_eq!(effects[1]["amount"]["kind"], property, "{oracle_text}");
        assert_eq!(&effects[1]["amount"]["object"], target);
    }
}

#[test]
fn committed_near_misses_report_the_responsible_local_automaton_state() {
    for (oracle_text, expected_state, found) in [
        (
            // A player has no power: the possessive must name an object.
            "Exile target creature. Its controller gains life equal to that player's power.",
            "numeric.expression.object-reference.nominal",
            Some("player's"),
        ),
        (
            // `Hero` is now a cataloged creature subtype; use an unknown noun.
            "Exile target widget. Its controller gains life equal to its power.",
            "selector.predicate.start",
            Some("widget"),
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure");
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("exile-anaphora-sequence")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(expected_state));
        assert_eq!(diagnostic.found.as_deref(), found);
    }
}

#[test]
fn optional_search_uses_the_distinct_native_zone_change_sequence() {
    let parsed = parse_spell(
        "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    );

    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let effects = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the distinct zone-change/search grammar should parse")["effects"]
        .as_array()
        .expect("ordered primitive effects");
    assert_eq!(effects[1]["kind"], "exilePermanent");
    assert_eq!(effects[2]["kind"], "optionalEffects");
}
