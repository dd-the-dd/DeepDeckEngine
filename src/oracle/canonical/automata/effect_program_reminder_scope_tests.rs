//! The optional-permission scope check ignores nested scopes: a `may` or an
//! `if` inside reminder text or a quoted ability is not a second frame of the
//! permission sentence (CR 207.2a, 113.1).
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Reminder Scope Probe".to_string(),
        type_line: "Creature — Human Druid".to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn states(parsed: &OracleCardParseResult) -> Vec<String> {
    parsed
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect()
}

#[test]
fn a_may_inside_the_mill_reminder_is_not_a_second_permission() {
    // Circle of the Land Druid, real Oracle text.
    let parsed = parse(
        "When this creature enters, you may mill four cards. (You may put the top four cards of your library into your graveyard.)",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let rule = parsed.abilities[0].rule.as_ref().unwrap().to_string();
    assert!(rule.contains("optional"), "{rule}");
}

#[test]
fn an_if_inside_a_reminder_does_not_reach_the_scope_check() {
    // Sibling: the reminder's own conditional (`if able`) stays inside it.
    for text in [
        "When this creature enters, you may mill two cards. (You may put the top two cards of your library into your graveyard.)",
        "When this creature enters, you may goad target creature. (Until your next turn, that creature attacks each combat if able and attacks a player other than you if able.)",
    ] {
        let parsed = parse(text);
        assert!(
            !states(&parsed)
                .iter()
                .any(|state| state == "program.optional.scope"),
            "{text}: {:#?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn a_root_conditional_or_second_permission_still_fails_the_scope() {
    for text in [
        "When this creature enters, you may draw a card if you control an artifact.",
        "When this creature enters, you may draw a card and you may gain 2 life. (Reminder.)",
    ] {
        let parsed = parse(text);
        assert_eq!(parsed.status, "unsupported", "{text}");
        assert!(
            states(&parsed)
                .iter()
                .any(|state| state == "program.optional.scope"),
            "{text}: {:#?}",
            parsed.diagnostics
        );
    }
}
