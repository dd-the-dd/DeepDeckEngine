use super::syntax::is_reminder_only_line;
use super::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle_text: &str) -> super::OracleCardParseResult {
    parse_oracle_card_native(OracleCardParseRequest {
        card_name: "Reminder Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn a_single_balanced_parenthetical_is_a_reminder_only_line() {
    assert!(is_reminder_only_line("(Theme color: {W})"));
    assert!(is_reminder_only_line("  (Theme color: {B}.) "));
    assert!(is_reminder_only_line("(Outer (nested) text.)"));
}

#[test]
fn text_around_a_parenthetical_is_not_a_reminder_only_line() {
    assert!(!is_reminder_only_line(
        "Flying (This creature can't be blocked.)"
    ));
    assert!(!is_reminder_only_line("(First.) Then more text."));
    assert!(!is_reminder_only_line("(First.) (Second.)"));
    assert!(!is_reminder_only_line("(Unbalanced"));
}

#[test]
fn an_unclaimed_reminder_only_line_yields_no_ability() {
    let parsed = parse("Avatar", "Flying\n(Theme color: {W})");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical")
    );
    assert_eq!(parsed.abilities.len(), 1);
}

#[test]
fn a_real_unsupported_ability_is_still_reported() {
    let parsed = parse("Avatar", "Frobnicate the quux.\n(Theme color: {W})");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].status, "unsupported");
}
