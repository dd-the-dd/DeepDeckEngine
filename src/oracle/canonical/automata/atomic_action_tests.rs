use crate::oracle::syntax::recognize_entities;

use super::{
    DiscardVerbForm, OracleTokenCursor, recognize_discard_action, recognize_sacrifice_action,
};

#[test]
fn cost_parent_supplies_a_typed_self_operand() {
    let tokens = recognize_entities("Sacrifice this creature");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let action = recognize_sacrifice_action(&mut cursor, |cursor| {
        cursor.consume("this")?;
        cursor.consume("creature")?;
        Ok("typed-self")
    })
    .expect("cost-shaped operand parses");

    assert_eq!(action.operand, "typed-self");
    cursor.finish().expect("the cost operand is complete");
}

#[test]
fn effect_parent_can_supply_an_anaphoric_operand() {
    let tokens = recognize_entities("Sacrifice it");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let action = recognize_sacrifice_action(&mut cursor, |cursor| {
        cursor.consume("it")?;
        Ok("bound-anaphor")
    })
    .expect("effect-shaped operand parses");

    assert_eq!(action.operand, "bound-anaphor");
    cursor.finish().expect("the effect operand is complete");
}

#[test]
fn another_action_is_a_precise_near_miss() {
    let tokens = recognize_entities("Destroy this creature");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let failure = recognize_sacrifice_action(&mut cursor, |_cursor| Ok(()))
        .expect_err("another action must not become sacrifice");

    assert_eq!(failure.state, "action.sacrifice.verb");
    assert_eq!(failure.actual.as_deref(), Some("Destroy"));
    assert!(failure.expected.contains("sacrifice"));
}

#[test]
fn missing_operand_fails_in_the_parent_child_without_partial_success() {
    let tokens = recognize_entities("Sacrifice");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let failure = recognize_sacrifice_action(&mut cursor, |cursor| {
        cursor.consume("this")?;
        Ok(())
    })
    .expect_err("a missing operand must remain visible");

    assert_eq!(failure.actual, None);
    assert_eq!(cursor.position(), 1);
}

#[test]
fn imperative_discard_delegates_the_cost_operand() {
    let tokens = recognize_entities("Discard this card");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let action = recognize_discard_action(&mut cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.consume("this")?;
        cursor.consume("card")?;
        Ok("source-card")
    })
    .expect("imperative discard parses");

    assert_eq!(action.operand, "source-card");
    cursor.finish().expect("the discard cost is complete");
}

#[test]
fn imperative_discard_accepts_the_one_card_cost_sibling() {
    let tokens = recognize_entities("Discard one card");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let action = recognize_discard_action(&mut cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.consume("one")?;
        cursor.consume("card")?;
        Ok(1)
    })
    .expect("one-card discard parses through the same action child");

    assert_eq!(action.operand, 1);
    cursor.finish().expect("the discard sibling is complete");
}

#[test]
fn third_person_discard_delegates_the_bound_effect_operand() {
    let tokens = recognize_entities("Discards that card");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let action = recognize_discard_action(
        &mut cursor,
        DiscardVerbForm::ThirdPersonSingular,
        |cursor| {
            cursor.consume("that")?;
            cursor.consume("card")?;
            Ok("chosen-card")
        },
    )
    .expect("third-person discard parses");

    assert_eq!(action.operand, "chosen-card");
    cursor.finish().expect("the discard effect is complete");
}

#[test]
fn discard_inflection_mismatch_is_a_precise_near_miss() {
    let tokens = recognize_entities("Discards this card");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let failure =
        recognize_discard_action(&mut cursor, DiscardVerbForm::Imperative, |_cursor| Ok(()))
            .expect_err("third-person syntax must not enter an imperative parent");

    assert_eq!(failure.state, "action.discard.verb");
    assert_eq!(failure.actual.as_deref(), Some("Discards"));
    assert!(failure.expected.contains("discard"));
}

#[test]
fn missing_discard_operand_remains_visible_to_the_parent() {
    let tokens = recognize_entities("Discard");
    let mut cursor = OracleTokenCursor::new("atomic-action-test", &tokens);

    let failure = recognize_discard_action(&mut cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.consume("a")?;
        Ok(())
    })
    .expect_err("a missing discard operand must not partially succeed");

    assert_eq!(failure.actual, None);
    assert_eq!(cursor.position(), 1);
}
