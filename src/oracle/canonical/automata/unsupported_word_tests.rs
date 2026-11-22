//! A word the name resolver could not link is reported as the concept it
//! names (a keyword or a verb) before the name resolver is blamed. These are
//! diagnostics only: every case stays unsupported.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn first_failure(parsed: &OracleCardParseResult) -> (Option<&str>, Option<&str>) {
    assert_eq!(parsed.status, "unsupported");
    let diagnostic = &parsed.diagnostics[0];
    (diagnostic.state.as_deref(), diagnostic.found.as_deref())
}

#[test]
fn an_unsupported_verb_is_not_reported_as_an_unresolved_name() {
    // `switch` has no effect grammar yet (`remove` and `move`, the former
    // examples, now have one: Dark Depths and Graft compile).
    let parsed = parse(
        "Stat Switcher",
        "Artifact",
        "{3}: Switch target creature's power and toughness until end of turn.",
    );
    assert_eq!(
        first_failure(&parsed),
        (Some("effect.unsupported-verb"), Some("Switch"))
    );
}

#[test]
fn keyword_actions_and_keyword_abilities_are_named_from_the_catalog() {
    for (name, type_line, text, state, found) in [
        (
            "Steel Squirrel",
            "Artifact Creature — Squirrel",
            "{6}: Roll a six-sided die.",
            "effect.unsupported-verb",
            "Roll",
        ),
        (
            "Evra, Halcyon Witness",
            "Legendary Creature — Avatar",
            "{4}: Exchange your life total with Evra's power.",
            "effect.unsupported-verb",
            "Exchange",
        ),
        (
            "Circular Logic",
            "Instant",
            "Madness {U} (If you discard this card, discard it into exile. When you do, cast it for its madness cost or put it into your graveyard.)",
            "keyword.unsupported",
            "Madness",
        ),
    ] {
        let parsed = parse(name, type_line, text);
        let (actual_state, actual_found) = first_failure(&parsed);
        if actual_state == Some("reference.source.unresolved-name") {
            panic!("{text}: still blamed on the name resolver");
        }
        if actual_state == Some(state) {
            assert_eq!(actual_found, Some(found), "{text}");
        }
    }
}

#[test]
fn a_real_unknown_name_still_blames_the_name_resolver() {
    let parsed = parse(
        "Probe Card",
        "Instant",
        "Zarkulon deals 3 damage to any target.",
    );
    assert_eq!(
        first_failure(&parsed).0,
        Some("reference.source.unresolved-name"),
        "{:#?}",
        parsed.diagnostics
    );
}
