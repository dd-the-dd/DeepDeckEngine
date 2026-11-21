use crate::oracle::syntax::recognize_entities;

use super::super::{LocalEffectNode, recognize_effect_sequence};
use super::{EffectDurationNode, ObjectCardType, OracleTokenCursor, TypeChangeRetention};

fn parse(text: &str) -> super::super::EffectSequenceNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.become-types", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None).expect(text)
}

#[test]
fn bare_subtype_replaces_and_additive_clause_retains() {
    let sequence = parse("Until end of turn, target creature becomes a Doctor.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one type change");
    };
    assert_eq!(
        node.subtypes
            .iter()
            .map(|subtype| subtype.ir_name())
            .collect::<Vec<_>>(),
        ["Doctor"]
    );
    assert!(node.card_types.is_empty());
    assert_eq!(node.retention, TypeChangeRetention::ReplacesSameClass);
    assert_eq!(
        node.duration,
        Some(EffectDurationNode::UntilEndOfCurrentTurn)
    );

    let sequence = parse("Target creature becomes an artifact in addition to its other types.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one type change");
    };
    assert_eq!(node.card_types, [ObjectCardType::Artifact]);
    assert_eq!(node.retention, TypeChangeRetention::InAdditionToOtherTypes);
    assert_eq!(node.duration, None);
}
