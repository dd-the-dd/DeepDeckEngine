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

#[test]
fn remove_from_combat_is_a_permanent_keyword_action() {
    for text in [
        "Remove target attacking or blocking creature from combat.",
        // Thaumatic Compass's shape: an untap, then the anaphor.
        "Untap target attacking creature an opponent controls and remove it from combat.",
        // An unused combination: a universal collection.
        "Remove all attacking creatures from combat.",
    ] {
        let sequence = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        let Some(LocalEffectNode::PermanentKeywordAction(node)) = sequence.effects.last() else {
            panic!("{text}: a removal from combat")
        };
        assert_eq!(
            node.action,
            PermanentKeywordAction::RemoveFromCombat,
            "{text}"
        );
    }
    // A counter removal keeps its own frame.
    let sequence = parse("Remove a +1/+1 counter from target creature.").unwrap();
    assert!(matches!(
        sequence.effects[0],
        LocalEffectNode::PutCounters(_)
    ));
    assert!(parse("Remove target creature from the battlefield.").is_err());
}

#[test]
fn distributed_counters_divide_among_one_declaration() {
    use super::super::{CounterChangeDirection, TargetCardinality};
    for (text, minimum, maximum) in [
        (
            "Distribute three +1/+1 counters among one, two, or three target creatures.",
            1,
            3,
        ),
        (
            "Distribute two -2/-1 counters among one or two target creatures.",
            1,
            2,
        ),
        (
            "Distribute four +1/+1 counters among any number of target creatures you control.",
            0,
            4,
        ),
        (
            "Distribute three +1/+1 counters among up to three other target creatures.",
            0,
            3,
        ),
    ] {
        let sequence = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        let LocalEffectNode::PutCounters(node) = &sequence.effects[0] else {
            panic!("{text}: a counter action")
        };
        assert_eq!(node.direction, CounterChangeDirection::Divided, "{text}");
        let PermanentOperandNode::Targets(targets) = &node.permanent else {
            panic!("{text}: declared targets")
        };
        assert_eq!(
            targets.slots[0].cardinality,
            TargetCardinality { minimum, maximum },
            "{text}"
        );
    }
    // Near misses: more required targets than counters, a non-consecutive
    // enumeration, a missing `among`.
    assert!(parse("Distribute two +1/+1 counters among three target creatures.").is_err());
    assert!(
        parse("Distribute three +1/+1 counters among one, three, or four target creatures.")
            .is_err()
    );
    assert!(parse("Distribute two +1/+1 counters on target creature.").is_err());
}

/// `can't be regenerated DURATION` (CR 701.15): a temporary marker on any
/// permanent subject, read with the modal's single form for every number.
#[test]
fn a_permanent_subject_cant_be_regenerated_for_a_duration() {
    for text in [
        // Furnace Brood.
        "Target creature can't be regenerated this turn.",
        // Sibling: a plural subject takes the same modal form.
        "Up to two target creatures can't be regenerated this turn.",
        // An unused combination: a counter then the anaphor's prohibition.
        "Put a -1/-1 counter on target creature. It can't be regenerated this turn.",
    ] {
        let sequence = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        let Some(LocalEffectNode::TemporaryKeywordGrant(node)) = sequence.effects.last() else {
            panic!("{text}: a temporary marker")
        };
        assert_eq!(
            node.keywords,
            vec![GrantedKeyword::CantBeRegenerated],
            "{text}"
        );
        assert_eq!(
            node.duration,
            EffectDurationNode::UntilEndOfCurrentTurn,
            "{text}"
        );
    }
}

/// Without a duration the prohibition is a destruction's amendment, owned
/// by the destroy frames: this frame makes no claim on it.
#[test]
fn an_undated_regeneration_prohibition_is_not_a_lasting_marker() {
    let failure = parse("Target creature can't be regenerated.")
        .expect_err("no lasting marker without a duration");
    assert_ne!(failure.state, "object-rule.cannot-be-regenerated.duration");
}
