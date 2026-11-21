use crate::oracle::canonical::machine::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{
    ActivationTimingNode, LinkedSetBinding, ObjectCardType, ObjectPredicate,
    recognize_linked_exile_return_activation,
};

const EXACT: &str = "{6}, {T}, Sacrifice this artifact: Put each creature card exiled with this artifact onto the battlefield under your control with a flying counter on it. Each of them is a 1/1 Spirit in addition to its other types. Activate only as a sorcery.";

#[test]
fn linked_return_composes_cost_set_move_characteristics_and_timing() {
    let tokens = recognize_entities(EXACT);
    let mut cursor = OracleTokenCursor::new("linked-exile-return-test", &tokens);
    let node = recognize_linked_exile_return_activation(&mut cursor).expect("ability parses");

    assert_eq!(node.costs.costs.len(), 3);
    assert_eq!(
        node.movement.cards.predicate,
        ObjectPredicate::CardType(ObjectCardType::Creature)
    );
    assert_eq!(node.movement.cards.source_type, ObjectCardType::Artifact);
    assert_eq!(node.movement.entering_counter.kind, "flying");
    assert_eq!(node.movement.entering_counter.count, 1);
    assert_eq!(node.characteristics.objects, LinkedSetBinding::MovedCards);
    assert_eq!(node.characteristics.power_toughness.power, 1);
    assert_eq!(node.characteristics.power_toughness.toughness, 1);
    assert!(node.characteristics.retain_existing_types);
    assert_eq!(node.timing, ActivationTimingNode::Sorcery);
}

#[test]
fn registered_types_subtypes_and_counter_names_are_reusable() {
    let text = "{3}, Sacrifice this enchantment: Put each artifact card exiled with this enchantment onto the battlefield under your control with a shadow counter on it. Each of them is a 2/3 Shark in addition to its other types. Activate only as a sorcery.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("linked-exile-return-test", &tokens);
    let node = recognize_linked_exile_return_activation(&mut cursor).expect("sibling parses");

    assert_eq!(
        node.movement.cards.predicate,
        ObjectPredicate::CardType(ObjectCardType::Artifact)
    );
    assert_eq!(node.movement.cards.source_type, ObjectCardType::Enchantment);
    assert_eq!(node.movement.entering_counter.kind, "shadow");
    assert_eq!(node.characteristics.power_toughness.power, 2);
    assert_eq!(node.characteristics.power_toughness.toughness, 3);
}

#[test]
fn malformed_retention_reports_the_local_state() {
    let text = EXACT.replace(
        "in addition to its other types",
        "instead of its other types",
    );
    let tokens = recognize_entities(&text);
    let mut cursor = OracleTokenCursor::new("linked-exile-return-test", &tokens);
    let failure = recognize_linked_exile_return_activation(&mut cursor)
        .expect_err("unsupported retention must fail");

    assert_eq!(failure.state, "characteristic.linked-set.retention");
    assert_eq!(failure.actual.as_deref(), Some("instead"));
}
