use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

#[test]
fn typed_exile_reuses_card_type_union_and_retains_graveyard_scope() {
    for (text, scope, predicate) in [
        (
            "Exile target land card from a graveyard",
            TargetCardSourceZone::SingleGraveyard,
            ObjectPredicate::CardType(ObjectCardType::Land),
        ),
        (
            "Exile target creature card from your graveyard",
            TargetCardSourceZone::ControllerGraveyard,
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ),
        (
            "Exile target instant or sorcery card from any graveyard",
            TargetCardSourceZone::AnyGraveyard,
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Instant),
                ObjectPredicate::CardType(ObjectCardType::Sorcery),
            ]),
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("exile.card.domain", &tokens);
        let TargetCardZoneChangeParseAttempt::Parsed(node) =
            try_recognize_exile_target_card_zone_change_effect(&mut cursor)
        else {
            panic!("shared card query failed: {text}")
        };
        assert_eq!(
            node.predicate,
            ObjectPredicate::Intersection(vec![
                ObjectPredicate::ObjectKind(ObjectKind::Card),
                predicate
            ])
        );
        assert_eq!(node.source_zone, scope);
        assert_eq!(node.cardinality, ExpressedTargetCardinality::EXACTLY_ONE);
        cursor.finish().unwrap();
    }
}
#[test]
fn card_domain_from_commits_but_permanent_candidate_does_not() {
    let tokens = recognize_entities("Exile target creature.");
    let mut cursor = OracleTokenCursor::new("exile.permanent.candidate", &tokens);
    assert!(matches!(
        try_recognize_exile_target_card_zone_change_effect(&mut cursor),
        TargetCardZoneChangeParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
    let tokens = recognize_entities("Exile target creature card from a hand.");
    let mut cursor = OracleTokenCursor::new("exile.card.invalidzone", &tokens);
    let TargetCardZoneChangeParseAttempt::FailedCommitted(failure) =
        try_recognize_exile_target_card_zone_change_effect(&mut cursor)
    else {
        panic!("typed source relation must commit its unsupported zone")
    };
    assert_eq!(failure.state, "card-target.source-capability");
}
#[test]
fn card_zone_atomic_parent_yields_multiaction_body_without_changing_atomic_success() {
    for text in [
        "{T}: Exile target card from any graveyard. You gain 2 life.",
        "{T}: Exile target card from any graveyard and you gain 2 life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("card.atomic.scope", &tokens);
        assert!(matches!(
            try_recognize_activated_card_zone_change(&mut cursor),
            ActivatedCardZoneChangeParseAttempt::RejectedCandidate(_)
        ));
        assert_eq!(cursor.position(), 0);
    }
    let tokens = recognize_entities("{T}: Exile target card from any graveyard.");
    let mut cursor = OracleTokenCursor::new("card.atomic.success", &tokens);
    assert!(matches!(
        try_recognize_activated_card_zone_change(&mut cursor),
        ActivatedCardZoneChangeParseAttempt::Parsed(_)
    ));
    cursor.finish().unwrap();
}
#[test]
fn activation_metadata_is_typed_and_remains_outside_the_two_effects() {
    let tokens = recognize_entities(
        "{T}: Exile target land card from a graveyard. Add one mana of any color. (Activate only as an instant.)",
    );
    let mut cursor = OracleTokenCursor::new("activation.metadata", &tokens);
    let ActivatedEffectSequenceParseAttempt::Parsed(node) =
        try_recognize_activated_effect_sequence_with_annotation(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
    else {
        panic!("targeted activation sequence should retain annotation")
    };
    assert_eq!(
        node.timing_annotation,
        Some(ActivationTimingAnnotation::InstantTimingExplanation)
    );
    assert!(matches!(
        node.effects.effects.as_slice(),
        [
            LocalEffectNode::TargetCardZoneChange(_),
            LocalEffectNode::AddMana(_)
        ]
    ));
    cursor.finish().unwrap();
}
#[test]
fn arbitrary_parentheses_and_timing_changes_are_not_ignored() {
    for text in [
        "{T}: Exile target land card from a graveyard. Add one mana of any color. (Activate only as a sorcery.)",
        "{T}: Exile target land card from a graveyard. Add one mana of any color. (Activate only as an instant.) You gain 2 life.",
        "{T}: Exile target land card from a graveyard. Add one mana of any color. (Activate only as an instant.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("activation.metadata.reject", &tokens);
        assert!(
            matches!(
                try_recognize_activated_effect_sequence_with_annotation(
                    &mut cursor,
                    &[],
                    None,
                    &EffectRecognitionContext::default()
                ),
                ActivatedEffectSequenceParseAttempt::FailedCommitted(_)
            ),
            "{text}"
        );
    }
}
