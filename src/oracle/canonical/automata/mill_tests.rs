use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalEffectNode, NumericAtom, NumericExpression, PlayerEffectSubject, PlayerPredicate,
    SingularPlayerAction, TargetSelector, recognize_effect_sequence,
};
use super::OracleTokenCursor;

#[test]
fn targeted_player_mill_is_an_independent_numeric_effect() {
    let tokens = recognize_entities("Target player mills four cards.");
    let mut cursor = OracleTokenCursor::new("test.mill-cards", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("mill effect parses");

    // Mill is a shared player verb: the declared player subject and the count
    // are independent children of the ordinary player action.
    let [LocalEffectNode::PlayerAction(effect)] = sequence.effects.as_slice() else {
        panic!("expected one shared player action");
    };
    assert_eq!(
        effect.action,
        SingularPlayerAction::Mill(NumericExpression::Atom(NumericAtom::Integer(4)))
    );
    let PlayerEffectSubject::DeclaredTarget { target, .. } = &effect.player else {
        panic!("mill subject declares its player target");
    };
    assert!(matches!(
        target.slots.as_slice(),
        [slot] if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
    ));
}

#[test]
fn mill_card_noun_must_agree_with_literal_count() {
    let tokens = recognize_entities("Target player mills one cards.");
    let mut cursor = OracleTokenCursor::new("test.invalid-mill-cards", &tokens);

    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("singular mill count needs singular card noun");

    assert_eq!(failure.actual.as_deref(), Some("cards"));
    assert!(failure.expected.contains("noun agreement"));
}

#[test]
fn a_target_subject_with_another_verb_is_not_a_mill_clause() {
    // Near miss: the target list reads further than any sibling, but the
    // clause never commits to mill, so its requirement is not the diagnostic.
    let tokens = recognize_entities("Target opponent reveals their hand.");
    let mut cursor = OracleTokenCursor::new("test.mill-near-miss", &tokens);

    let outcome = super::try_recognize_mill_cards_effect(&mut cursor);
    assert!(matches!(outcome, super::ChildOutcome::NoMatch(_)));
    assert_eq!(cursor.position(), 0, "a rejected transaction does not move");

    let failure = super::recognize_mill_cards_effect(&mut cursor).unwrap_err();
    assert_eq!(failure.state, "effect.clause/subject.target.verb.reveal");
    assert_eq!(failure.actual.as_deref(), Some("reveals"));
    assert!(!failure.expected.contains("exactly one target player"));
}

#[test]
fn a_mill_clause_without_a_player_subject_is_committed() {
    // Sibling: subject and lemma are read, so the mill frame owns the clause
    // and reports its own requirement.
    let tokens = recognize_entities("Target creature mills two cards.");
    let mut cursor = OracleTokenCursor::new("test.mill-committed", &tokens);

    let super::ChildOutcome::Committed(failure) =
        super::try_recognize_mill_cards_effect(&mut cursor)
    else {
        panic!("subject and lemma commit the mill frame");
    };
    assert_eq!(failure.state, "effect.mill-cards.player");
    assert_eq!(cursor.position(), 0);
}

#[test]
fn a_target_subject_that_stops_before_its_verb_reports_the_unread_word() {
    // Sibling near miss: the subject reader stops at `and` (a target list
    // coordinates its branches with `or`); the diagnostic names that unread
    // word, not the verb found later in the sentence.
    let tokens = recognize_entities("Target creature and player gets +2/+2 until end of turn.");
    let mut cursor = OracleTokenCursor::new("test.mill-subject", &tokens);

    let failure = super::recognize_mill_cards_effect(&mut cursor).unwrap_err();
    assert_eq!(failure.state, "effect.clause/subject.target.unread.and");
    assert_eq!(failure.actual.as_deref(), Some("and"));
}
