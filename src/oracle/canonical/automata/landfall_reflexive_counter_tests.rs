use crate::oracle::syntax::{recognize_oracle_entities, resolve_counter_name_spans};

use super::*;
use crate::oracle::canonical::{NumericExpression, ObjectCardType, OracleTokenCursor};

fn recognize(text: &str) -> MachineResult<LabeledControlledEntryReflexiveCounterNode> {
    let mut tokens = recognize_oracle_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("labeled-controlled-entry-reflexive-counter", &tokens);
    recognize_labeled_controlled_entry_reflexive_counter(&mut cursor, &[], None)
}

#[test]
fn recognizes_controlled_entry_counter_threshold_target_and_keyword_children() {
    let node = recognize(
        "Landfall \u{2014} Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.",
    )
    .expect("the complete labeled reflexive counter grammar should parse");

    assert!(node.event.object.guarantees_card_type(ObjectCardType::Land));
    assert_eq!(node.antecedent.counter, "quest");
    assert_eq!(
        node.antecedent.count,
        NumericExpression::Atom(NumericAtom::Integer(1))
    );
    assert_eq!(node.followup.condition.counter, "quest");
    assert_eq!(
        node.followup.condition.operator,
        NumericComparisonOperator::GreaterThanOrEqual
    );
    assert_eq!(node.followup.condition.threshold, NumericAtom::Integer(4));
    assert_eq!(node.followup.placement.counter, "+1/+1");
    assert_eq!(node.followup.grant.keywords, vec![GrantedKeyword::Trample]);
    assert_eq!(
        node.followup.grant.duration,
        EffectDurationNode::UntilEndOfCurrentTurn
    );
}

#[test]
fn sibling_varies_label_event_source_counter_threshold_target_and_keyword() {
    let node = recognize(
        "Will of the council \u{2014} Whenever an artifact you control enters, put two charge counters on this artifact. When you do, if it has three or fewer charge counters on it, put a shield counter on target artifact creature you control. It gains flying until end of turn.",
    )
    .expect("the grammar should be driven by typed children rather than exact card text");

    assert!(
        node.event
            .object
            .guarantees_card_type(ObjectCardType::Artifact)
    );
    assert_eq!(node.antecedent.counter, "charge");
    assert_eq!(
        node.antecedent.count,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
    assert_eq!(node.followup.condition.threshold, NumericAtom::Integer(3));
    assert_eq!(
        node.followup.condition.operator,
        NumericComparisonOperator::LessThanOrEqual
    );
    assert_eq!(node.followup.placement.counter, "shield");
    assert_eq!(node.followup.grant.keywords, vec![GrantedKeyword::Flying]);
}

#[test]
fn wrong_reflexive_player_is_a_committed_structured_failure() {
    let failure = recognize(
        "Landfall \u{2014} Whenever a land you control enters, put a quest counter on this enchantment. When an opponent does, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.",
    )
    .expect_err("a different reflexive actor must not be approximated");

    assert_eq!(
        failure.state,
        "trigger.labeled-reflexive-counter.reflexive-player"
    );
    assert_eq!(failure.actual.as_deref(), Some("an"));
}

#[test]
fn a_collection_does_not_declare_a_singular_source_for_it() {
    let failure = recognize(
        "Landfall \u{2014} Whenever a land you control enters, put a quest counter on each enchantment you control. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.",
    )
    .expect_err("a collection must not be coerced into the source antecedent");

    assert_eq!(failure.state, "condition.source-counter-threshold.source");
    assert_eq!(failure.expected, "a source binding for `it`");
}

#[test]
fn unsupported_counter_comparator_stays_visible() {
    let failure = recognize(
        "Landfall \u{2014} Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or exactly quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.",
    )
    .expect_err("an unsupported comparator must remain explicit");

    assert_eq!(
        failure.state,
        "condition.source-counter-threshold.comparator-direction"
    );
    assert_eq!(failure.actual.as_deref(), Some("exactly"));
}
