use crate::oracle::syntax::recognize_entities;

use super::super::{LocalPlayerReference, NumericAtom, NumericExpression};
use super::{OracleTokenCursor, recognize_surveil_effect, recognize_surveil_reminder};

fn recognize(text: &str) -> super::super::super::MachineResult<super::SurveilEffectNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("surveil-effect-test", &tokens);
    let effect = recognize_surveil_effect(&mut cursor, None)?;
    cursor.finish()?;
    Ok(effect)
}

#[test]
fn literal_surveil_is_an_independent_controller_effect() {
    let effect = recognize("surveil 2").expect("surveil effect parses");

    assert_eq!(effect.player, LocalPlayerReference::Controller);
    assert_eq!(
        effect.count,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
}

#[test]
fn variable_surveil_reuses_the_numeric_expression_machine() {
    let effect = recognize("surveil X").expect("variable surveil parses");

    assert_eq!(
        effect.count,
        NumericExpression::Atom(NumericAtom::VariableX)
    );
}

#[test]
fn surveil_rejects_non_numeric_and_non_positive_counts() {
    let unknown = recognize("surveil many").expect_err("unknown count must fail");
    assert_eq!(unknown.actual.as_deref(), Some("many"));

    let zero = recognize("surveil 0").expect_err("zero count must fail");
    assert_eq!(
        zero.expected,
        "a positive surveil count or variable expression"
    );
}

#[test]
fn canonical_singular_and_plural_reminders_share_one_typed_machine() {
    for (count, reminder) in [
        (
            1,
            "(Look at the top card of your library. You may put it into your graveyard.)",
        ),
        (
            2,
            "(Look at the top two cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
        ),
    ] {
        let tokens = recognize_entities(reminder);
        let mut cursor = OracleTokenCursor::new("surveil-reminder-test", &tokens);
        recognize_surveil_reminder(
            &mut cursor,
            &NumericExpression::Atom(NumericAtom::Integer(count)),
        )
        .expect("canonical reminder parses");
        cursor.finish().expect("reminder consumes its input");
    }
}

#[test]
fn reminder_count_must_match_the_effect_count() {
    let tokens = recognize_entities(
        "(Look at the top three cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
    );
    let mut cursor = OracleTokenCursor::new("surveil-reminder-test", &tokens);

    let failure = recognize_surveil_reminder(
        &mut cursor,
        &NumericExpression::Atom(NumericAtom::Integer(2)),
    )
    .expect_err("mismatched reminder must fail");

    assert_eq!(
        failure.expected,
        "the same literal count as the surveil effect"
    );
}
