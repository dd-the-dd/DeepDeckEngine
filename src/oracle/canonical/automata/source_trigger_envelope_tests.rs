use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn source_transition_envelope_exports_kind_and_leaves_the_body_unread() {
    for (text, expected) in [
        (
            "When this creature enters, you may draw a card.",
            SourceTriggerEventKind::Entered,
        ),
        (
            "Whenever this artifact dies, draw a card.",
            SourceTriggerEventKind::Died,
        ),
        (
            "When this creature enters the battlefield, draw a card.",
            SourceTriggerEventKind::Entered,
        ),
        (
            "Whenever this land becomes tapped, draw a card.",
            SourceTriggerEventKind::Tapped,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("source-envelope-test", &tokens);
        let SourceTriggerEnvelopeAttempt::Recognized(node) =
            try_recognize_source_trigger_envelope(&mut cursor, &[], None)
        else {
            panic!("source envelope must parse: {text}")
        };
        assert_eq!(node.event, expected);
        assert_eq!(node.source, LocalObjectReference::SourceSelf);
        assert_eq!(node.body_start, cursor.position());
        assert!(matches!(cursor.peek(), Some("you" | "draw")));
    }
}

#[test]
fn unrelated_events_are_not_claimed_and_compound_events_remain_unsupported() {
    for text in [
        "Whenever a creature enters, draw a card.",
        "When this creature attacks, draw a card.",
        "This creature enters tapped.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("source-envelope-near-miss", &tokens);
        assert!(
            matches!(
                try_recognize_source_trigger_envelope(&mut cursor, &[], None),
                SourceTriggerEnvelopeAttempt::NotClaimed
            ),
            "{text}"
        );
        assert_eq!(
            cursor.position(),
            0,
            "soft mismatch must restore the caller: {text}"
        );
        assert_eq!(cursor.fail("probe restored state").state, "start");
    }
}

#[test]
fn an_unclaimed_child_preserves_a_nonzero_parent_checkpoint() {
    let tokens = recognize_entities("Draw a card. When this creature attacks, draw a card.");
    let mut cursor = OracleTokenCursor::new("source-envelope-parent", &tokens);
    cursor.consume_phrase(&["Draw", "a", "card", "."]).unwrap();
    cursor.enter("parent.next-ability");
    let position = cursor.position();
    let trace = cursor.transitions().to_vec();
    assert!(matches!(
        try_recognize_source_trigger_envelope(&mut cursor, &[], None),
        SourceTriggerEnvelopeAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), position);
    assert_eq!(cursor.transitions().len(), trace.len() + 1);
    assert_eq!(&cursor.transitions()[..trace.len()], trace);
    assert_eq!(cursor.transitions().last().unwrap().state, "rollback");
    assert_eq!(cursor.transitions().last().unwrap().token_index, position);
    assert_eq!(
        cursor.fail("probe restored state").state,
        "parent.next-ability"
    );
}

#[test]
fn tapped_transition_requires_its_complete_state_and_separator() {
    for (text, expected) in [
        (
            "Whenever this land becomes untapped, draw a card.",
            "trigger.source-program.tapped-transition",
        ),
        (
            "Whenever this land becomes tapped for mana, draw a card.",
            "trigger.source-program.separator",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("source-tapped-boundary", &tokens);
        let SourceTriggerEnvelopeAttempt::Unpromoted(failure) =
            try_recognize_source_trigger_envelope(&mut cursor, &[], None)
        else {
            panic!("unsupported source transition must remain explicit: {text}");
        };
        assert_eq!(failure.state, expected);
    }
}

#[test]
fn damage_anaphor_requires_the_typed_source_binding_from_its_parent() {
    let tokens = recognize_entities("it deals 2 damage to you.");
    for bound in [false, true] {
        let mut cursor = OracleTokenCursor::new("source-bound-damage", &tokens);
        let source = LocalObjectReference::SourceSelf;
        let result = super::super::effects::recognize_damage_effect_with_source_antecedent(
            &mut cursor,
            &[],
            None,
            bound.then_some(super::super::PermanentEffectReference::Source(source)),
        );
        if bound {
            let super::super::LocalEffectNode::DealDamage(node) = result.unwrap() else {
                panic!("damage child must return typed damage");
            };
            assert_eq!(
                node.source,
                crate::oracle::canonical::automata::PermanentOperandNode::Reference(source)
            );
            assert_eq!(cursor.peek(), Some("."));
        } else {
            assert_eq!(
                result.unwrap_err().state,
                "reference.source-or-anaphor.start"
            );
        }
    }
}

#[test]
fn owner_event_and_local_scopes_are_distinct_damage_source_proofs() {
    use super::super::{
        EffectRecognitionContext, ObservedSourceRelationship, PermanentEffectReference as R,
        PermanentSubjectScope as Scope, TargetBinding, TypedObservedEventObject as O,
    };
    let source = LocalObjectReference::SourceSelf;
    let owner = EffectRecognitionContext::for_source(source);
    // The legacy resolver is intentionally unchanged for unmigrated consumers.
    assert_eq!(
        super::super::permanent_subject::permanent_antecedent(&owner),
        Some(R::Source(source))
    );
    let event = owner.clone().with_source_event_subject(source);
    let mut local_source = owner.clone();
    local_source.permanent_subject_scope = Scope::Local(Some(R::Source(source)));
    let mut target = event.clone();
    target.permanent_subject_scope = Scope::Local(Some(R::Target(TargetBinding {
        effect_index: 0,
        slot_index: 0,
    })));
    let mut observed = event.clone();
    observed.permanent_subject_scope = Scope::Local(Some(R::Observed(O::EnteringPermanent)));
    let mut exhausted = event.clone();
    exhausted.permanent_subject_scope = Scope::Local(None);
    let observed_parent = owner
        .clone()
        .with_observed_event_object(O::EnteringPermanent);
    let mut same_entering = observed.clone();
    same_entering.observed_source_relationship = ObservedSourceRelationship::SameEnteringObject;
    for (label, context, valid) in [
        ("owner-only initial memory", owner, false),
        ("proved event source", event, true),
        ("proved lexical source", local_source, true),
        // The local target is the referent: it, not the owner, deals damage.
        ("local target supersedes event", target, false),
        ("unaliased local observed object", observed, false),
        ("locally exhausted", exhausted, false),
        ("other entering object", observed_parent, false),
        (
            "genuine same-entering alias plus event proof",
            same_entering,
            true,
        ),
    ] {
        let tokens = recognize_entities("it deals 1 damage to you.");
        let mut cursor = OracleTokenCursor::new("source-proof-context", &tokens);
        let result =
            super::super::recognize_effect_sequence_with_context(&mut cursor, &[], None, &context);
        if valid {
            let node = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
            assert!(
                matches!(&node.effects[0], super::super::LocalEffectNode::DealDamage(damage) if damage.source == crate::oracle::canonical::automata::PermanentOperandNode::Reference(source))
            );
        } else {
            let owner_source =
                crate::oracle::canonical::automata::PermanentOperandNode::Reference(source);
            assert!(
                result.as_ref().map_or(true, |node| !matches!(
                    &node.effects[0],
                    super::super::LocalEffectNode::DealDamage(damage) if damage.source == owner_source
                )),
                "unproved source must not become owner: {label}"
            );
            if label == "local target supersedes event" {
                let node = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                assert!(matches!(
                    &node.effects[0],
                    super::super::LocalEffectNode::DealDamage(damage)
                        if damage.source == crate::oracle::canonical::automata::PermanentOperandNode::BoundReference(
                            R::Target(TargetBinding { effect_index: 0, slot_index: 0 })
                        )
                ));
            }
        }
    }
}

#[test]
fn lexical_results_rebind_or_exhaust_the_proved_event_subject() {
    use super::super::EffectRecognitionContext;
    let source = LocalObjectReference::SourceSelf;
    let context = EffectRecognitionContext::for_source(source).with_source_event_subject(source);
    for (text, valid) in [
        (
            "This artifact gets +1/+1 until end of turn. It deals 1 damage to you.",
            true,
        ),
        ("You gain 1 life. It deals 1 damage to you.", true),
        ("Add {R}. It deals 1 damage to you.", true),
        (
            "Each opponent loses 1 life. It deals 1 damage to you.",
            true,
        ),
        (
            "Each opponent discards a card. It deals 1 damage to you.",
            false,
        ),
        // A target referent is an object damage source, never the owner.
        (
            "Target creature gets +1/+1 until end of turn. It deals 1 damage to you.",
            true,
        ),
        ("Untap target creature. It deals 1 damage to you.", true),
        (
            "Creatures you control get +1/+1 until end of turn. It deals 1 damage to you.",
            false,
        ),
        ("Draw a card. It deals 1 damage to you.", false),
        ("Sacrifice a creature. It deals 1 damage to you.", false),
        ("Create a Treasure token. It deals 1 damage to you.", false),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("source-proof-lexical-update", &tokens);
        let result =
            super::super::recognize_effect_sequence_with_context(&mut cursor, &[], None, &context);
        assert_eq!(result.is_ok(), valid, "{text}: {result:?}");
        if let Ok(node) = &result
            && text.contains("target")
        {
            assert!(
                matches!(
                    node.effects.last(),
                    Some(super::super::LocalEffectNode::DealDamage(damage))
                        if matches!(
                            damage.source,
                            crate::oracle::canonical::automata::PermanentOperandNode::BoundReference(_)
                        )
                ),
                "{text}: the target referent is the damage source"
            );
        }
    }
}
