use super::*;
use crate::oracle::canonical::engine_gap::EngineGap;
use crate::oracle::syntax::recognize_entities;

fn sink_failure(
    text: &str,
    consumed: &[&str],
    frame: Option<&'static str>,
    sink: &'static str,
) -> (Vec<RecognizedEntity>, MachineFailure) {
    let tokens = recognize_entities(text);
    let failure = {
        let mut cursor = OracleTokenCursor::new("test.failure-owner", &tokens);
        for word in consumed {
            cursor.consume(word).expect("the probe prefix is present");
        }
        if let Some(frame) = frame {
            cursor.enter(frame);
        }
        cursor.enter(sink);
        cursor.fail("the child's own expectation")
    };
    (tokens, failure)
}

#[test]
fn clause_heads_name_their_deepest_component() {
    for (text, component) in [
        ("You may pay {1}.", "subject.you.may.verb.pay"),
        ("When that creature dies, draw a card.", "introducer.when"),
        (
            "That creature perpetually gains flying.",
            "subject.reference.verb.gain",
        ),
        (
            "Creatures your opponents control get -1/-1 until end of turn.",
            "subject.object.verb.get",
        ),
        ("the top card of your library.", "subject.definite.no-verb"),
        ("Look at the top card of your library.", "verb-unknown.look"),
        (
            "Switch target creature's power and toughness.",
            "verb-unknown.switch",
        ),
        (
            "Players can't cast spells.",
            "subject.player.cannot.verb.cast",
        ),
        (
            "Target opponent reveals their hand.",
            "subject.target.verb.reveal",
        ),
    ] {
        let tokens = recognize_entities(text);
        assert_eq!(
            diagnose_clause_head(&tokens, 0).component,
            component,
            "{text}"
        );
    }
}

#[test]
fn a_relative_clause_verb_is_not_the_main_verb() {
    // Sibling of the table above: `you cast` and `you control` are relative
    // clauses inside the subject; the main verb is the one after them.
    let tokens = recognize_entities("Red spells you cast cost {1} less to cast.");
    let head = diagnose_clause_head(&tokens, 0);
    assert_eq!(head.component, "subject.object.verb-unknown.cost");
    assert_eq!(head.verb_index, Some(4));
}

#[test]
fn every_sink_is_owned_and_keeps_its_child_evidence() {
    let cases: [(&str, &[&str], Option<&'static str>, &'static str, &str); 7] = [
        (
            "Look at the top card of your library.",
            &[],
            None,
            "effect.start",
            "effect.clause/verb-unknown.look",
        ),
        (
            "discard your hand",
            &["discard"],
            Some("effect.test-discard.count"),
            "numeric.atom.start",
            "effect.test-discard.count/numeric.object-phrase",
        ),
        (
            "destroy each opponent",
            &["destroy", "each"],
            Some("effect.test-destroy.object"),
            "selector.predicate.start",
            "effect.test-destroy.object/object-predicate.player-noun",
        ),
        (
            "target creature becomes red",
            &["target", "creature"],
            Some("effect.test-target.subject"),
            "target-list.complete",
            "effect.test-target.subject/target-list.verb",
        ),
        (
            "you commit a crime",
            &["you"],
            None,
            "event.ordinal-card-drawn.action",
            "event.clause/subject.you.verb-unknown.commit",
        ),
        (
            "Zarkulon deals 3 damage",
            &[],
            Some("effect.test-damage.source"),
            "reference.source.unresolved-name",
            "effect.test-damage.source/source-name.unlinked-name",
        ),
        (
            "destroy target creature unless you pay 1 life",
            &["destroy", "target", "creature"],
            Some("effect.test-destroy.complete"),
            "sequence.connector-or-end",
            "effect.test-destroy.complete/modifier.unless",
        ),
    ];
    for (text, consumed, frame, sink, owner) in cases {
        let (tokens, failure) = sink_failure(text, consumed, frame, sink);
        let owned = assign_failure_owner(failure.clone(), &tokens);
        assert_eq!(owned.owner, Some(owner), "{text}");
        assert!(!is_sink_state(owner), "{text}: an owner is never a sink");
        // The child's precise evidence is unchanged; only the owner is added.
        assert_eq!(
            MachineFailure {
                owner: None,
                ..owned
            },
            failure,
            "{text}"
        );
    }
}

#[test]
fn owned_failures_and_engine_gaps_are_never_rewritten() {
    let tokens = recognize_entities("Whenever this creature becomes untapped, draw a card.");
    let mut cursor = OracleTokenCursor::new("test.failure-owner", &tokens);
    cursor.enter("trigger.source-event.verb-support");
    let gap = cursor.engine_gap(EngineGap::EventMatcher, "a runtime self matcher");
    assert_eq!(gap.state, "engine.gap.event-matcher");
    assert!(gap.is_engine_gap());
    assert!(
        gap.trace
            .iter()
            .any(|transition| transition.state == "trigger.source-event.verb-support"),
        "the frame that found the gap stays in the trace"
    );
    assert_eq!(assign_failure_owner(gap.clone(), &tokens), gap);
    assert_eq!(gap.owner, None);

    cursor.enter("effect.draw-cards.count");
    let grammar = cursor.fail("a count");
    assert_eq!(assign_failure_owner(grammar.clone(), &tokens), grammar);
}

#[test]
fn composed_states_are_interned_once() {
    let first = intern_state("effect.clause/verb-unknown.look".to_string());
    let second = intern_state("effect.clause/verb-unknown.look".to_string());
    assert!(std::ptr::eq(first, second));
}
