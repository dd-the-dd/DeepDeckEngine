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
fn a_committed_local_family_serializes_its_precise_machine_failure() {
    let oracle = "Example Source deals damage to any target and you gain 3 life.";
    let parsed = parse("Example Source", oracle);
    let diagnostic = &parsed.abilities[0].diagnostics[0];

    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("effect-sequence"));
    assert_eq!(diagnostic.state.as_deref(), Some("numeric.atom.start"));
    assert_eq!(diagnostic.found.as_deref(), Some("damage"));
    assert_eq!(
        diagnostic.expected.as_deref(),
        Some("integer, number word, or X")
    );
    let start = diagnostic.span_start.expect("failure span start");
    let end = diagnostic.span_end.expect("failure span end");
    assert_eq!(&oracle[start..end], "damage");
    assert!(diagnostic.token_index.is_some());
    assert!(!diagnostic.trace.is_empty());
    assert!(diagnostic.trace.len() <= 32);
}

#[test]
fn legacy_unsupported_keeps_the_legacy_diagnostic_shape() {
    // Every spell sentence is now claimed by the shared contextual program,
    // so an unclaimed static sentence witnesses the legacy shape.
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some("Players can't search libraries.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.abilities[0].recognizer,
        deepdeck_oracle::oracle::RecognizerProvenance::Unclaimed
    );
    let diagnostic = &parsed.abilities[0].diagnostics[0];

    assert_eq!(diagnostic.code, "unsupported_oracle_ability");
    assert!(diagnostic.automaton.is_none());
    assert!(diagnostic.state.is_none());
    assert!(diagnostic.token_index.is_none());
    assert!(diagnostic.trace.is_empty());
}

#[test]
fn an_unlinked_unknown_name_preserves_the_complete_group_and_span() {
    // `Source` is a vocabulary lexeme; an all-unknown name forms one group.
    let oracle = "Unknown Widget deals 3 damage to any target and you gain 3 life.";
    let parsed = parse("Different Source", oracle);
    let diagnostic = &parsed.abilities[0].diagnostics[0];

    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("reference.source.unresolved-name")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("Unknown Widget"));
    assert_eq!(diagnostic.span_start, Some(0));
    assert_eq!(diagnostic.span_end, Some(14));
    assert_eq!(diagnostic.token_index, Some(0));
}
