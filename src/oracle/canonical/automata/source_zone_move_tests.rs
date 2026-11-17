use crate::oracle::syntax::recognize_entities;

use super::super::{
    EffectRecognitionContext, LocalObjectReference, MoveSourceEffectNode, SourceFaceChange,
    SourceMoveController, SourceZone,
};
use super::{OracleTokenCursor, recognize_source_zone_move_effect};

#[test]
fn source_exile_and_anaphoric_transformed_return_are_separate_moves() {
    let context = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);

    let exile_tokens = recognize_entities("exile this creature");
    let mut exile_cursor = OracleTokenCursor::new("move-source-test", &exile_tokens);
    let exile = recognize_source_zone_move_effect(&mut exile_cursor, &[], None, &context)
        .expect("source exile parses");
    assert_eq!(
        exile,
        MoveSourceEffectNode {
            source: LocalObjectReference::SourceSelf,
            from: SourceZone::Battlefield,
            to: SourceZone::Exile,
            face_change: SourceFaceChange::Preserve,
            controller: SourceMoveController::Preserve,
        }
    );

    let return_tokens =
        recognize_entities("return her to the battlefield transformed under her owner's control");
    let mut return_cursor = OracleTokenCursor::new("move-source-test", &return_tokens);
    let returned = recognize_source_zone_move_effect(&mut return_cursor, &[], None, &context)
        .expect("anaphoric transformed return parses");
    assert_eq!(
        returned,
        MoveSourceEffectNode {
            source: LocalObjectReference::SourceSelf,
            from: SourceZone::Exile,
            to: SourceZone::Battlefield,
            face_change: SourceFaceChange::Transform,
            controller: SourceMoveController::Owner,
        }
    );
}

#[test]
fn return_anaphor_requires_a_parent_source_binding() {
    let tokens =
        recognize_entities("return it to the battlefield transformed under its owner's control");
    let mut cursor = OracleTokenCursor::new("move-source-test", &tokens);

    let failure = recognize_source_zone_move_effect(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .expect_err("an unbound source pronoun must fail");

    assert_eq!(failure.actual.as_deref(), Some("it"));
    assert!(failure.expected.contains("source binding"));
}
