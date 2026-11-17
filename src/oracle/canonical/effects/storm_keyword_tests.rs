//! Storm (CR 702.40) is a parameterless keyword ability of the spell. Its
//! reminder is validated token by token; the lowered IR is the same
//! `keywordAbility(storm)` the engine's storm-copy runtime test executes.
use serde_json::json;

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

#[test]
fn storm_with_either_printed_reminder_lowers_to_the_runtime_keyword() {
    let storm =
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"storm"}});
    for (name, type_line, text) in [
        (
            "Grapeshot",
            "Sorcery",
            "Grapeshot deals 1 damage to any target.\nStorm (When you cast this spell, copy it for each spell cast before it this turn. You may choose new targets for the copies.)",
        ),
        (
            "Empty the Warrens",
            "Sorcery",
            "Create two 1/1 red Goblin creature tokens.\nStorm (When you cast this spell, copy it for each spell cast before it this turn.)",
        ),
        // Unused combination: a bare keyword on an instant with another effect.
        ("Probe", "Instant", "Storm\nDraw a card."),
    ] {
        let parsed = parse(name, type_line, text);
        assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
        assert!(
            parsed
                .abilities
                .iter()
                .any(|ability| ability.rule.as_ref() == Some(&storm)),
            "{text}"
        );
    }
}

#[test]
fn storm_reminder_with_an_unregistered_clause_is_an_explicit_failure() {
    // Tempest Technique's `Copies become tokens.` is not this reminder grammar.
    let parsed = parse(
        "Probe",
        "Sorcery",
        "Storm (When you cast this spell, copy it for each spell cast before it this turn. Copies become tokens.)",
    );
    assert_ne!(parsed.status, "canonical");
}
