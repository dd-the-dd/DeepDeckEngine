use super::*;
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

#[test]
fn independent_counter_keyword_and_stats_actions_share_one_typed_target_subject() {
    let mut tokens = recognize_entities(
        "Put a +1/+1 counter on target creature. It gains indestructible until end of turn. It gets +2/+1 until end of turn. Put a charge counter on it.",
    );
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("bound-permanent-actions", &tokens);
    let node = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    cursor.finish().unwrap();
    let reference = PermanentEffectReference::Target(TargetBinding {
        effect_index: 0,
        slot_index: 0,
    });
    assert!(
        matches!(&node.effects[0], LocalEffectNode::PutCounters(node) if matches!(node.permanent, PermanentOperandNode::Targets(_)))
    );
    assert!(
        matches!(&node.effects[1], LocalEffectNode::TemporaryKeywordGrant(node) if node.objects == PermanentOperandNode::BoundReference(reference))
    );
    assert!(
        matches!(&node.effects[2], LocalEffectNode::ModifyStats(node) if node.targets == PermanentOperandNode::BoundReference(reference))
    );
    assert!(
        matches!(&node.effects[3], LocalEffectNode::PutCounters(node) if node.permanent == PermanentOperandNode::BoundReference(reference))
    );
}

#[test]
fn permanent_subject_scope_never_borrows_a_player_plural_or_missing_antecedent() {
    for text in [
        "It gains indestructible until end of turn.",
        "Put a +1/+1 counter on target player. It gains indestructible until end of turn.",
        "Put a +1/+1 counter on two target creatures. It gains indestructible until end of turn.",
        "Put a +1/+1 counter on target creature. Target player gains 1 life. It gains indestructible until end of turn.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("bound-permanent-subject-negative", &tokens);
        assert!(
            recognize_effect_sequence(&mut cursor, &[], None).is_err(),
            "{text}"
        );
    }
    for (context, expected) in [
        (
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
                .with_source_event_subject(LocalObjectReference::SourceSelf),
            PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        ),
        (
            EffectRecognitionContext::default()
                .with_observed_event_object(TypedObservedEventObject::EnteringPermanent),
            PermanentOperandNode::BoundReference(PermanentEffectReference::Observed(
                TypedObservedEventObject::EnteringPermanent,
            )),
        ),
        (
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
                .with_observed_event_object(TypedObservedEventObject::EnteringPermanent),
            PermanentOperandNode::BoundReference(PermanentEffectReference::Observed(
                TypedObservedEventObject::EnteringPermanent,
            )),
        ),
        (
            EffectRecognitionContext {
                observed_source_relationship: ObservedSourceRelationship::SameEnteringObject,
                ..EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
                    .with_source_event_subject(LocalObjectReference::SourceSelf)
                    .with_observed_event_object(TypedObservedEventObject::EnteringPermanent)
            },
            PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        ),
    ] {
        let tokens = recognize_entities("It gains flying until end of turn.");
        let mut cursor = OracleTokenCursor::new("bound-permanent-role", &tokens);
        let node =
            recognize_effect_sequence_with_context(&mut cursor, &[], None, &context).unwrap();
        assert!(
            matches!(&node.effects[0], LocalEffectNode::TemporaryKeywordGrant(node) if node.objects == expected)
        );
    }
}

#[test]
fn object_target_domain_is_owned_by_selector_not_repeated_in_each_characteristic() {
    for text in [
        "Target Merfolk",
        "Target Ninja or Turtle",
        "Target red permanent",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("permanent-subject-domain", &tokens);
        let subject = recognize_contextual_permanent_operand(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .unwrap();
        cursor.finish().unwrap();
        assert!(
            matches!(subject, PermanentOperandNode::Targets(_)),
            "{text}"
        );
    }
    for text in [
        "Target card",
        "Target spell",
        "Target player",
        "Target creature or player",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("permanent-subject-domain-negative", &tokens);
        assert!(
            recognize_contextual_permanent_operand(
                &mut cursor,
                &[],
                None,
                &EffectRecognitionContext::default()
            )
            .is_err(),
            "{text}"
        );
    }
}
