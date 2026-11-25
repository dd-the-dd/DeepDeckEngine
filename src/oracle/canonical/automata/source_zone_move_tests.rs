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
            tapped: false,
            attached_to: None,
            entering_counters: None,
            attacking: None,
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
            tapped: false,
            attached_to: None,
            entering_counters: None,
            attacking: None,
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

#[test]
fn a_source_card_returns_from_its_graveyard_to_the_battlefield_optionally_tapped() {
    let context = EffectRecognitionContext::default();
    for (text, tapped) in [
        (
            "return this card from your graveyard to the battlefield",
            false,
        ),
        (
            "return this card from your graveyard to the battlefield tapped",
            true,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("move-source-test", &tokens);
        let returned = recognize_source_zone_move_effect(&mut cursor, &[], None, &context)
            .expect("a graveyard source return parses");
        assert_eq!(
            returned,
            MoveSourceEffectNode {
                source: LocalObjectReference::SourceSelf,
                from: SourceZone::Graveyard,
                to: SourceZone::Battlefield,
                face_change: SourceFaceChange::Preserve,
                controller: SourceMoveController::Preserve,
                tapped,
                attached_to: None,
                entering_counters: None,
                attacking: None,
            },
            "{text}"
        );
        assert_eq!(cursor.position(), tokens.len(), "{text}");
    }
}

/// The shared entry-counter reader of card moves is the source move's
/// adjunct: the counters are part of the entry (CR 122.6), after `tapped`
/// as written, with a literal or the article for one counter.
#[test]
fn a_graveyard_source_return_enters_with_the_counters_it_names() {
    let context = EffectRecognitionContext::default();
    for (text, tapped, counter, count) in [
        (
            // Gastal Thrillroller.
            "return this card from your graveyard to the battlefield with a finality counter on it",
            false,
            "finality",
            1_i64,
        ),
        (
            // Retrofitted Transmogrant (sibling: tapped, plural counters).
            "return this card from your graveyard to the battlefield tapped with two +1/+1 counters on it",
            true,
            "+1/+1",
            2_i64,
        ),
    ] {
        // The pipeline resolves counter-name spans before the grammar runs.
        let mut tokens = recognize_entities(text);
        crate::oracle::syntax::resolve_counter_name_spans(&mut tokens);
        let mut cursor = OracleTokenCursor::new("move-source-test", &tokens);
        let returned = recognize_source_zone_move_effect(&mut cursor, &[], None, &context)
            .expect("a return with entry counters parses");
        assert_eq!(returned.tapped, tapped, "{text}");
        let counters = returned
            .entering_counters
            .as_ref()
            .expect("the entry counters are read");
        assert_eq!(counters.counter.as_str(), counter, "{text}");
        assert_eq!(
            counters.count,
            super::super::NumericExpression::from(count),
            "{text}"
        );
        assert_eq!(cursor.position(), tokens.len(), "{text}");
    }
    // Near miss: the number of the counter noun must agree with its amount.
    let tokens = recognize_entities(
        "return this card from your graveyard to the battlefield with two +1/+1 counter on it",
    );
    let mut cursor = OracleTokenCursor::new("move-source-test", &tokens);
    let failure = recognize_source_zone_move_effect(&mut cursor, &[], None, &context)
        .expect_err("a counter noun disagreeing with its amount fails");
    assert_eq!(
        failure.state,
        "collection.destination.entering-counter-noun"
    );
}

/// `attached to OBJECT` is the shared permanent operand: a declared target
/// here, a bound reference or the source in other abilities.
#[test]
fn a_graveyard_source_return_reads_the_permanent_it_enters_attached_to() {
    let context = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
    let tokens = recognize_entities(
        "return this card from your graveyard to the battlefield attached to target creature you control",
    );
    let mut cursor = OracleTokenCursor::new("move-source-test", &tokens);
    let returned = recognize_source_zone_move_effect(&mut cursor, &[], None, &context)
        .expect("the attached return parses");
    assert_eq!(
        (returned.from, returned.to),
        (SourceZone::Graveyard, SourceZone::Battlefield)
    );
    assert!(matches!(
        returned.attached_to,
        Some(super::super::PermanentOperandNode::Targets(_))
    ));
    assert_eq!(cursor.position(), tokens.len());
}
