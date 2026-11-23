use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

const COMPOUND_EXILE: &str = "Target opponent exiles a creature they control and their graveyard.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Strategic Betrayal".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn targeted_player_compound_exile_is_native_and_primitive() {
    let parsed = parse(COMPOUND_EXILE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );

    let rule = ability.rule.as_ref().expect("native rule");
    assert_eq!(rule["kind"], "spellAbility");
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .expect("target declaration");
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["id"], "targetOpponent");
    assert_eq!(decisions[0]["candidates"]["kind"], "players");

    let effects = rule["effects"].as_array().expect("primitive effects");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "choosePermanents");
    assert_eq!(effects[0]["player"]["id"], "targetOpponent");
    assert_eq!(effects[0]["candidates"]["where"]["value"], "Creature");
    assert_eq!(effects[1]["kind"], "exileTogether");
    assert_eq!(
        effects[1]["objects"]["sets"][0]["decisionId"],
        "controlledPermanentToExile"
    );
    assert_eq!(
        effects[1]["objects"]["sets"][1]["zone"]["kind"],
        "graveyard"
    );
    assert_eq!(
        effects[1]["objects"]["sets"][1]["zone"]["player"]["id"],
        "targetOpponent"
    );
}

#[test]
fn card_type_and_player_selector_are_reusable_parameters() {
    let parsed = parse("Target player exiles an artifact they control and their graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native sibling rule");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPlayer");
    assert_eq!(
        rule["effects"][0]["candidates"]["where"]["value"],
        "Artifact"
    );
}

#[test]
fn malformed_family_members_fail_in_the_responsible_local_automaton() {
    for (oracle_text, expected_state, expected_found) in [
        (
            "Target opponent exiles a creature they control or their graveyard.",
            "compound-exile.object-connector",
            "or",
        ),
        (
            "Target opponent exiles a creature they control and their library.",
            "zone.possessive-player.kind",
            "library",
        ),
        (
            "Target opponent exiles an instant they control and their graveyard.",
            "object.controlled-permanent.predicate",
            "they",
        ),
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let diagnostic = ability.diagnostics.first().expect("committed failure");
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("player-compound-exile"),
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
fn unrelated_player_exile_phrases_remain_outside_the_promoted_boundary() {
    // The shared program owns the unrelated phrase and rejects it explicitly.
    let parsed = parse("Target opponent exiles a card from their graveyard.");
    assert_eq!(parsed.status, "unsupported");
    let ability = &parsed.abilities[0];
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    // The effect dispatcher reads the declared subject and routes only a
    // target player who mills to the mill frame; any other verb fails at the
    // shared clause state naming the lemma and the subject kind (S3).
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("clause.verb.unsupported")
    );
    assert!(
        ability.diagnostics[0]
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("a declared target player")),
        "{:#?}",
        ability.diagnostics
    );
}
