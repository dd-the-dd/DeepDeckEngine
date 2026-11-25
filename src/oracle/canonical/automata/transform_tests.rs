use crate::oracle::syntax::recognize_entities;

use super::super::{LocalObjectReference, PermanentOperandNode, TransformPermanentEffectNode};
use super::{OracleTokenCursor, recognize_transform_source_effect};

#[test]
fn recognizes_transform_this_permanent_as_one_atomic_source_action() {
    let tokens = recognize_entities("Transform this permanent");
    let mut cursor = OracleTokenCursor::new("test.transform", &tokens);

    let effect =
        recognize_transform_source_effect(&mut cursor, &[], None).expect("source transform parses");

    assert_eq!(
        effect,
        TransformPermanentEffectNode {
            permanent: PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        }
    );
    cursor.finish().expect("transform consumes its full range");
}

#[test]
fn source_object_noun_is_generic() {
    for text in ["Transform this creature", "Transform this token"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.transform", &tokens);

        let effect = recognize_transform_source_effect(&mut cursor, &[], None)
            .unwrap_or_else(|failure| panic!("{text} failed: {failure:?}"));

        assert_eq!(
            effect.permanent,
            PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
        );
        cursor.finish().expect("sibling consumes its full range");
    }
}

#[test]
fn target_form_is_not_stolen_by_the_source_only_machine() {
    let tokens = recognize_entities("Transform target permanent");
    let mut cursor = OracleTokenCursor::new("test.transform", &tokens);

    let failure = recognize_transform_source_effect(&mut cursor, &[], None)
        .expect_err("target transform needs a target-aware parent");

    assert_eq!(failure.automaton, "test.transform");
    assert_eq!(failure.state, "reference.source.start");
    assert_eq!(failure.actual.as_deref(), Some("target"));
    assert!(failure.expected.contains("current-card name"));
}
