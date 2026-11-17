use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{ObjectCardType, ObjectPredicate};
use super::*;

#[test]
fn commander_presence_reuses_the_controlled_permanent_count_node() {
    let tokens = recognize_entities("If you control a commander,");
    let mut cursor = OracleTokenCursor::new("test.controlled-permanent-presence", &tokens);

    let condition = recognize_controlled_permanent_presence_condition(&mut cursor)
        .expect("commander presence condition parses");

    assert_eq!(
        condition,
        LocalConditionNode::ControlledPermanentCount(ControlledPermanentCountConditionNode {
            player: LocalPlayerReference::Controller,
            threshold: 1,
            direction: ControlledCountDirection::OrMore,
            predicate: ObjectPredicate::IsCommander,
            exclude_source: false,
        })
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn legendary_creature_presence_is_a_distinct_composed_predicate() {
    let tokens = recognize_entities("If you control a legendary creature,");
    let mut cursor = OracleTokenCursor::new("test.controlled-permanent-presence", &tokens);

    let condition = recognize_controlled_permanent_presence_condition(&mut cursor)
        .expect("legendary creature presence condition parses");
    let LocalConditionNode::ControlledPermanentCount(condition) = condition else {
        panic!("presence lowers through the count primitive");
    };

    assert_ne!(condition.predicate, ObjectPredicate::IsCommander);
    assert!(
        condition
            .predicate
            .guarantees_card_type(ObjectCardType::Creature)
    );
}
