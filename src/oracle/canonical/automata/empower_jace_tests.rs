use crate::oracle::syntax::recognize_entities;

use super::*;

const REMINDER_TAIL: &str = "If you don't control one, first create a blue Jace planeswalker token with \"[\u{2212}1]: Surveil 1\" and \"[\u{2212}3]: Draw a card.\")";

fn recognize(text: &str) -> MachineResult<EmpowerJaceEffectNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("empower-jace-test", &tokens);
    recognize_empower_jace_action(&mut cursor, None)
}

/// Returns the number of tokens left after the reminder.
fn reminder(count: &NumericExpression, text: &str) -> MachineResult<usize> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("empower-jace-reminder-test", &tokens);
    recognize_empower_jace_reminder(&mut cursor, count)?;
    Ok(tokens.len() - cursor.position())
}

#[test]
fn literal_and_variable_counts_use_the_shared_numeric_expression() {
    assert_eq!(
        recognize("Empower Jace 2").unwrap().count,
        NumericExpression::from(2)
    );
    assert_eq!(
        recognize("empower Jace X").unwrap().count,
        NumericExpression::Atom(NumericAtom::VariableX)
    );
}

#[test]
fn the_action_requires_the_jace_identity_and_a_count() {
    assert_eq!(
        recognize("Empower Chandra 2")
            .unwrap_err()
            .actual
            .as_deref(),
        Some("Chandra")
    );
    assert!(recognize("Empower Jace").is_err());
}

#[test]
fn reminder_repeats_the_count_and_matches_the_catalog_token() {
    let plural = format!("(Put two loyalty counters on a Jace token you control. {REMINDER_TAIL}");
    assert_eq!(reminder(&NumericExpression::from(2), &plural).unwrap(), 0);
    let single = format!("(Put a loyalty counter on a Jace token you control. {REMINDER_TAIL}");
    assert_eq!(reminder(&NumericExpression::from(1), &single).unwrap(), 0);
    let variable =
        format!("(Put that many loyalty counters on a Jace token you control. {REMINDER_TAIL}");
    assert_eq!(
        reminder(&NumericExpression::Atom(NumericAtom::VariableX), &variable).unwrap(),
        0
    );
}

#[test]
fn reminder_near_misses_fail_explicitly() {
    let wrong_count =
        format!("(Put three loyalty counters on a Jace token you control. {REMINDER_TAIL}");
    assert!(reminder(&NumericExpression::from(2), &wrong_count).is_err());
    let altered = "(Put two loyalty counters on a Jace token you control. If you don't control one, first create a blue Jace planeswalker token with \"[\u{2212}1]: Draw a card\" and \"[\u{2212}3]: Draw a card.\")";
    let failure = reminder(&NumericExpression::from(2), altered).unwrap_err();
    assert_eq!(failure.state, "effect.empower-jace.reminder.abilities");
}

#[test]
fn the_gate_opens_once_the_catalog_token_compiles_completely() {
    // The Jace token's printed loyalty abilities (with their reminder) now
    // compile through the shared grammar, so the action is executable. The
    // closed gate itself is proven below with an incomplete definition.
    assert!(catalog_token_definition_is_complete());
    let tokens = recognize_entities("Empower Jace 2");
    let mut cursor = OracleTokenCursor::new("empower-jace-gate-test", &tokens);
    recognize_empower_jace_effect(&mut cursor, None).expect("a complete catalog token");
}

#[test]
fn a_token_definition_is_complete_only_when_every_ability_compiles() {
    let complete = "{T}, Sacrifice this token: Add one mana of any color.";
    assert!(token_definition_is_complete(
        "Treasure",
        "Token Artifact - Treasure",
        complete
    ));
    // The printed Jace loyalty abilities do not all compile yet.
    let printing = named_token_printing("Jace").unwrap().unwrap();
    assert_eq!(
        token_definition_is_complete(
            "Jace",
            printing.type_line.as_deref().unwrap(),
            printing.oracle_text.as_deref().unwrap()
        ),
        catalog_token_definition_is_complete()
    );
    let partial = format!("{complete}\nThis ability text is not supported at all.");
    assert!(!token_definition_is_complete(
        "Treasure",
        "Token Artifact - Treasure",
        &partial
    ));
}
