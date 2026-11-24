use super::super::super::OracleTokenCursor;
use super::super::{
    EffectDurationNode, EffectRecognitionContext, EffectSequenceNode, GrantedKeyword,
    LocalEffectNode, PermanentKeywordAction, PermanentOperandNode,
    recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> Result<EffectSequenceNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-rule-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
}

#[test]
fn a_declared_or_bound_permanent_phases_out() {
    for text in [
        "Target creature phases out.",
        "Target creature you control phases out.",
        // An unused combination: a counter then the anaphor phases out.
        "Put a +1/+1 counter on target creature. It phases out.",
    ] {
        let sequence = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        let Some(LocalEffectNode::PermanentKeywordAction(node)) = sequence.effects.last() else {
            panic!("{text}: a phase-out action")
        };
        assert_eq!(node.action, PermanentKeywordAction::PhaseOut, "{text}");
        assert!(node.quantity.is_none());
    }
    let sequence = parse("Up to two target creatures phase out.").unwrap();
    let LocalEffectNode::PermanentKeywordAction(node) = &sequence.effects[0] else {
        panic!("a phase-out action")
    };
    assert!(matches!(node.targets, PermanentOperandNode::Targets(_)));
}

#[test]
fn phasing_agrees_with_its_subject_and_names_its_direction() {
    assert!(parse("Target creature phase out.").is_err());
    assert!(parse("Up to two target creatures phases out.").is_err());
    // `phases in` has no one-shot reading here.
    assert!(parse("Target creature phases in.").is_err());
}

#[test]
fn no_combat_damage_is_a_temporary_marker_with_its_duration() {
    let sequence = parse("Target creature assigns no combat damage this turn.").unwrap();
    let LocalEffectNode::TemporaryKeywordGrant(grant) = &sequence.effects[0] else {
        panic!("a temporary marker")
    };
    assert_eq!(grant.keywords, [GrantedKeyword::AssignsNoCombatDamage]);
    assert_eq!(grant.duration, EffectDurationNode::UntilEndOfCurrentTurn);
    // The rule needs its duration.
    assert!(parse("Target creature assigns no combat damage.").is_err());
}
