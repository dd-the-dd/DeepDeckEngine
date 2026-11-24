use crate::oracle::syntax::recognize_entities;

use super::super::{LocalActivationCostNode, LocalEffectNode};
use super::{
    ActivatedEffectSequenceParseAttempt, EffectRecognitionContext,
    try_recognize_activated_effect_sequence_with_context,
};
use super::{OracleTokenCursor, recognize_activated_effect_sequence};

#[test]
fn generic_activation_composes_costs_and_independent_effects() {
    let tokens = recognize_entities("{2}{B}: Add {B}. Then you gain 1 life.");
    let mut cursor = OracleTokenCursor::new("test.activated-effect-sequence", &tokens);

    let activation = recognize_activated_effect_sequence(&mut cursor, &[], None)
        .expect("activated effect sequence parses");

    assert!(matches!(
        activation.costs.costs.as_slice(),
        [LocalActivationCostNode::PayMana(_)]
    ));
    assert!(matches!(
        activation.effects.effects.as_slice(),
        [LocalEffectNode::AddMana(_), LocalEffectNode::GainLife(_)]
    ));
}

#[test]
fn generic_activation_composes_transform_source_without_card_specific_text() {
    let tokens = recognize_entities("{2}: Transform this token.");
    let mut cursor = OracleTokenCursor::new("test.activated-effect-sequence", &tokens);

    let activation = recognize_activated_effect_sequence(&mut cursor, &[], None)
        .expect("activated source transform parses");

    assert!(matches!(
        activation.costs.costs.as_slice(),
        [LocalActivationCostNode::PayMana(_)]
    ));
    assert!(matches!(
        activation.effects.effects.as_slice(),
        [LocalEffectNode::TransformPermanent(_)]
    ));
}

#[test]
fn typed_activation_preserves_the_compatibility_wrappers_nodes_and_trace() {
    for text in [
        "{2}{B}: Add {B}. Then you gain 1 life.",
        "{2}: Transform this token.",
        "{4}, {T}: Target creature can't be blocked this turn.",
    ] {
        let tokens = recognize_entities(text);
        let mut compatibility = OracleTokenCursor::new("activation-parity", &tokens);
        let expected = recognize_activated_effect_sequence(&mut compatibility, &[], None)
            .expect("compatibility activation parses");
        let mut transactional = OracleTokenCursor::new("activation-parity", &tokens);
        let ActivatedEffectSequenceParseAttempt::Parsed(actual) =
            try_recognize_activated_effect_sequence_with_context(
                &mut transactional,
                &[],
                None,
                &EffectRecognitionContext::default(),
            )
        else {
            panic!("typed activation must preserve a supported program: {text}");
        };
        assert_eq!(actual, expected);
        assert_eq!(transactional.transitions(), compatibility.transitions());
    }
}

#[test]
fn typed_activation_rolls_back_unsupported_costs_and_independent_suffixes() {
    let tokens =
        recognize_entities("{R}, Sacrifice an artifact: Target creature can't block this turn.");
    let mut cursor = OracleTokenCursor::new("activation-selected-cost-positive", &tokens);
    let ActivatedEffectSequenceParseAttempt::Parsed(activation) =
        try_recognize_activated_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
    else {
        panic!("selected sacrifice has an existing supported cost child")
    };
    assert!(matches!(
        activation.costs.costs.as_slice(),
        [
            LocalActivationCostNode::PayMana(_),
            LocalActivationCostNode::SacrificeObject(_)
        ]
    ));
    assert!(matches!(
        activation.effects.effects.as_slice(),
        [LocalEffectNode::TemporaryKeywordGrant(_)]
    ));
    cursor.finish().unwrap();
    // `Activate only` is typed ability metadata, never an effect or a rollback.
    let tokens = recognize_entities(
        "{T}: Target creature can't be blocked this turn. Activate only as a sorcery and only once each turn.",
    );
    let mut cursor = OracleTokenCursor::new("activation-restrictions", &tokens);
    let ActivatedEffectSequenceParseAttempt::Parsed(restricted) =
        try_recognize_activated_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
    else {
        panic!("activation restrictions are owned by the activated parent");
    };
    assert_eq!(
        restricted.restrictions,
        super::super::ActivationRestrictionsNode {
            timing: Some(super::super::ActivationTiming::Sorcery),
            limit: Some(super::super::ActivationCountLimit::PerTurn(1)),
            ..Default::default()
        }
    );
    cursor.finish().unwrap();
    // A counted discard is now a typed cost slot; a reveal cost still has no
    // activation schema, so the whole activation rolls back.
    for text in ["{R}, Reveal a card from your hand: Target creature can't block this turn."] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("activation-rollback", &tokens);
        let initial_state = cursor.fail("checkpoint observation").state;
        assert!(matches!(
            try_recognize_activated_effect_sequence_with_context(
                &mut cursor,
                &[],
                None,
                &EffectRecognitionContext::default(),
            ),
            ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(_)
        ));
        assert_eq!(cursor.position(), 0);
        assert_eq!(cursor.fail("rollback observation").state, initial_state);
    }
}

#[test]
fn typed_activation_retains_a_childs_explicit_commit_without_position_heuristics() {
    let tokens = recognize_entities("{T}: Target creature can't be blocked this combat.");
    let mut cursor = OracleTokenCursor::new("activation-committed", &tokens);
    let ActivatedEffectSequenceParseAttempt::FailedCommitted(failure) =
        try_recognize_activated_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
    else {
        panic!("a committed combat child failure must not become a support limit");
    };
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");
}
