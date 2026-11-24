use crate::oracle::syntax::recognize_entities;

use super::*;
use crate::oracle::canonical::{
    BooleanConditionNode, EffectRecognitionContext, NumericComparisonOperator,
    controlled_permanent_count,
};

fn guard(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<Option<SentenceConditionNode>> {
    let effects = EffectRecognitionContext::default();
    recognize_trigger_guard(
        cursor,
        &SentenceConditionContext {
            name_links: &[],
            name_failure: None,
            effects: &effects,
            card_target: None,
            card_focus: false,
            departed_targets: &[],
        },
    )
}

#[test]
fn optional_guard_does_not_consume_an_effect_body() {
    let tokens = recognize_entities("draw a card.");
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    assert_eq!(guard(&mut cursor).unwrap(), None);
    assert_eq!(cursor.position(), 0);
}

#[test]
fn presence_and_object_union_leave_the_parent_comma() {
    for text in [
        "if you control an artifact, draw a card.",
        "if you control an artifact or enchantment, draw a card.",
        "if you control a creature that's red or green, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
        let node = guard(&mut cursor).expect(text).expect(text);
        let Some((_, operator, threshold, _)) = controlled_permanent_count(&node) else {
            panic!("one typed presence atom: {text}");
        };
        assert_eq!(threshold, 1);
        assert_eq!(operator, NumericComparisonOperator::GreaterThanOrEqual);
        assert_eq!(cursor.peek(), Some(","), "{text}");
        cursor.consume(",").unwrap();
        assert_eq!(cursor.peek(), Some("draw"), "{text}");
    }
}

#[test]
fn full_subject_boolean_conditions_preserve_precedence() {
    let tokens = recognize_entities(
        "if you control an artifact or you control an enchantment and you control two or more creatures, draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let Some(BooleanConditionNode::Or(branches)) = guard(&mut cursor).unwrap() else {
        panic!("OR owns the expression root");
    };
    assert_eq!(branches.len(), 2);
    assert!(matches!(&branches[1], BooleanConditionNode::And(items) if items.len() == 2));
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn serial_boolean_commas_require_a_later_explicit_connector() {
    let tokens = recognize_entities(
        "if you control an artifact, you control an enchantment, and you control two or more creatures, draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let Some(BooleanConditionNode::And(items)) = guard(&mut cursor).unwrap() else {
        panic!("three complete condition atoms form one conjunction");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(cursor.peek(), Some(","));
    cursor.consume(",").unwrap();
    assert_eq!(cursor.peek(), Some("draw"));
}

#[test]
fn an_effect_with_its_own_and_is_not_a_condition_list_item() {
    let tokens =
        recognize_entities("if you control an artifact, you gain 1 life and you gain 2 life.");
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let Some(BooleanConditionNode::Atom(_)) = guard(&mut cursor).unwrap() else {
        panic!("the single presence condition remains atomic");
    };
    cursor.consume(",").unwrap();
    assert_eq!(cursor.peek(), Some("you"));
    assert_eq!(cursor.peek_ahead(1), Some("gain"));
}

#[test]
fn bare_serial_comma_does_not_invent_boolean_meaning() {
    let tokens =
        recognize_entities("if you control an artifact, you control an enchantment, draw a card.");
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let Some(BooleanConditionNode::Atom(_)) = guard(&mut cursor).unwrap() else {
        panic!("an unresolved comma remains the parent's boundary");
    };
    cursor.consume(",").unwrap();
    assert_eq!(cursor.peek(), Some("you"));
    assert_eq!(cursor.peek_ahead(1), Some("control"));
}

#[test]
fn prefix_negation_is_typed_and_repeated_not_cancels() {
    for (text, negated) in [
        ("if not you control an artifact, draw a card.", true),
        ("if not not you control an artifact, draw a card.", false),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
        let node = guard(&mut cursor).unwrap().unwrap();
        assert_eq!(matches!(node, BooleanConditionNode::Not(_)), negated);
        assert_eq!(cursor.peek(), Some(","));
    }
}

/// `you don't control a|another QUERY` is the negated control proposition:
/// the absence count of the shared controlled-permanent query, never a
/// denied optional payment.
#[test]
fn a_negated_control_proposition_is_the_absence_count() {
    for text in [
        "if you don't control an artifact, draw a card.",
        "if you don't control another Dinosaur, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
        let Some(BooleanConditionNode::Atom(_)) = guard(&mut cursor).unwrap() else {
            panic!("{text}: one absence atom");
        };
        assert_eq!(cursor.peek(), Some(","), "{text}");
    }
}

#[test]
fn unsupported_atoms_counts_and_scopes_fail_explicitly() {
    for text in [
        "if you control X or more creatures, draw a card.",
        "if you control an instant, draw a card.",
        "if you control a creature you don't control, draw a card.",
        "if you control an artifact and a creature, draw a card.",
        "if you control an artifact or banana, draw a card.",
        "if you control an artifact or, draw a card.",
        "if you control an artifact draw a card.",
        // Near miss: a negated count threshold is not the absence count.
        "if you don't control two or more creatures, draw a card.",
        "if you don't control 2 or more creatures, draw a card.",
        "if you don't control at least two creatures, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
        assert!(guard(&mut cursor).is_err(), "{text}");
    }
}

#[test]
fn the_guard_allow_list_refuses_a_previous_action_outcome() {
    // Near miss: `if you can't` is an action outcome of an effect program; an
    // intervening `if` has no earlier action to read.
    let tokens = recognize_entities("if you can't, draw a card.");
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let failure = guard(&mut cursor).expect_err("no earlier action exists");
    assert_eq!(failure.state, "condition.allow-list");
}

#[test]
fn a_history_with_an_opponent_actor_composes_in_a_guard() {
    // Unprinted combination: the generic event history with a player
    // relation, conjoined with a controlled-permanent comparison.
    let tokens = recognize_entities(
        "if an opponent searched their library this turn and you control an artifact, draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("test.trigger-guard", &tokens);
    let Some(BooleanConditionNode::And(items)) = guard(&mut cursor).unwrap() else {
        panic!("two atoms form one conjunction");
    };
    assert!(matches!(
        &items[0],
        BooleanConditionNode::Atom(super::super::ConditionAtom::EventHistory(history))
            if history.pattern.event == super::super::HistoryEvent::LibrarySearched
                && history.pattern.player == super::super::HistoryPlayer::Opponent
    ));
    assert_eq!(cursor.peek(), Some(","));
}
