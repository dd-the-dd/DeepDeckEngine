use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::super::{CardColor, CreatureSubtype, NumericAtom, recognize_mobilize_keyword};

fn parse(text: &str) -> super::super::MobilizeKeywordNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("mobilize-test", &tokens);
    recognize_mobilize_keyword(&mut cursor, &[], None).expect("Mobilize parses")
}

#[test]
fn reminder_exports_a_typed_combat_token_batch() {
    let node = parse(
        "Mobilize 2 (Whenever this creature attacks, create two tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)",
    );
    assert_eq!(node.quantity, 2);
    let reminder = node.reminder.expect("typed reminder");
    assert_eq!(reminder.tokens.creation.quantity, NumericAtom::Integer(2));
    assert_eq!(reminder.tokens.creation.token.colors, [CardColor::Red]);
    assert_eq!(
        reminder.tokens.creation.token.subtypes,
        [CreatureSubtype::Warrior]
    );
    assert!(reminder.tokens.tapped);
    assert!(reminder.tokens.attacking);
}

#[test]
fn reminder_does_not_guess_a_capitalized_token_subtype() {
    let tokens = recognize_entities(
        "Mobilize 2 (Whenever this creature attacks, create two tapped and attacking 1/1 red Imaginarytype creature tokens. Sacrifice them at the beginning of the next end step.)",
    );
    let mut cursor = OracleTokenCursor::new("mobilize-test", &tokens);
    let failure =
        recognize_mobilize_keyword(&mut cursor, &[], None).expect_err("unknown subtype fails");
    assert_eq!(
        failure.state,
        "effect.create-token.subtype-list.item-or-creature"
    );
    assert_eq!(failure.actual.as_deref(), Some("Imaginarytype"));
}

#[test]
fn bare_and_singular_mobilize_are_generalized() {
    assert_eq!(parse("Mobilize 3").quantity, 3);
    assert_eq!(
        parse(
            "Mobilize 1 (Whenever this creature attacks, create one tapped and attacking 1/1 red Warrior creature token. Sacrifice them at the beginning of the next end step.)",
        )
        .quantity,
        1
    );
}

#[test]
fn reminder_quantity_mismatch_is_a_committed_failure() {
    let tokens = recognize_entities(
        "Mobilize 2 (Whenever this creature attacks, create three tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)",
    );
    let mut cursor = OracleTokenCursor::new("mobilize-test", &tokens);
    let failure =
        recognize_mobilize_keyword(&mut cursor, &[], None).expect_err("mismatch must fail");
    assert_eq!(failure.state, "keyword.mobilize.reminder.quantity-match");
    assert!(failure.expected.contains("equal"));
}
