use crate::oracle::canonical::ObjectCardType;
use crate::oracle::syntax::recognize_oracle_entities;

use super::{
    ObjectControllerScope, ObjectPredicate, OracleTokenCursor, PermanentDiedEventBranchNode,
    recognize_permanent_died_event,
};

fn parse(text: &str) -> super::MachineResult<super::PermanentDiedEventNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("permanent-died-event-test", &tokens);
    recognize_permanent_died_event(&mut cursor, &[], None)
}

#[test]
fn recognizes_explicit_source_union_without_collapsing_source_to_creature_filter() {
    let event = parse("this creature or another creature dies")
        .expect("source plus filtered alternative should parse");
    assert!(matches!(
        event.alternatives.as_slice(),
        [
            PermanentDiedEventBranchNode::Source(_),
            PermanentDiedEventBranchNode::Filtered {
                predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
                controller: None,
                exclude_source: true,
            }
        ]
    ));
}

#[test]
fn recognizes_controlled_other_creature_as_a_filtered_event() {
    let event =
        parse("another creature you control dies").expect("controlled creature death should parse");
    assert!(matches!(
        event.alternatives.as_slice(),
        [PermanentDiedEventBranchNode::Filtered {
            controller: Some(ObjectControllerScope::AbilityController),
            exclude_source: true,
            ..
        }]
    ));
}

#[test]
fn accepts_three_explicit_event_alternatives_without_an_arbitrary_cap() {
    let event = parse("this creature or this artifact or this enchantment dies")
        .expect("the event parent should loop over every explicit alternative");
    assert_eq!(event.alternatives.len(), 3);
    assert!(
        event
            .alternatives
            .iter()
            .all(|branch| matches!(branch, PermanentDiedEventBranchNode::Source(_)))
    );
}

#[test]
fn rejects_an_event_without_the_dies_action() {
    let failure = parse("another creature you control leaves")
        .expect_err("another event must not be approximated as dying");
    assert_eq!(failure.state, "event.permanent-died.action");
    assert_eq!(failure.actual.as_deref(), Some("leaves"));
}
