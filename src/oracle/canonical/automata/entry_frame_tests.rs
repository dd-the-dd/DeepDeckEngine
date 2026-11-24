use super::super::{ObjectCardType, ObjectControllerScope};
use super::*;
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

fn read(text: &str) -> ChildOutcome<EntryFrameNode> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("entry-frame-test", &tokens);
    try_recognize_entry_frame(&mut cursor, &[], None)
}

fn matched(text: &str) -> EntryFrameNode {
    match read(text) {
        ChildOutcome::Matched(node) => node,
        ChildOutcome::Committed(failure) | ChildOutcome::NoMatch(failure) => {
            panic!("{text}: {failure}")
        }
    }
}

#[test]
fn entering_state_composes_tapped_and_counters() {
    let node = matched("This land enters tapped with two charge counters on it.");
    assert!(node.tapped);
    assert_eq!(node.counters.len(), 1);
    assert_eq!(node.counters[0].counter.as_str(), "charge");
    assert_eq!(
        node.counters[0].count,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
    assert!(node.copy.is_none() && node.choice.is_none());
}

#[test]
fn the_older_battlefield_complement_is_the_same_entry_verb() {
    let node = matched("This land enters the battlefield tapped.");
    assert!(node.tapped && node.counters.is_empty());
}

#[test]
fn a_stun_counter_reminder_restates_the_counter_rule() {
    let node = matched(
        "This creature enters tapped with three stun counters on it. (If it would become untapped, remove a stun counter from it instead.)",
    );
    assert!(node.tapped);
    assert_eq!(node.counters[0].counter.as_str(), "stun");
}

#[test]
fn a_second_sentence_choice_belongs_to_the_same_replacement() {
    let node = matched("This land enters tapped. As it enters, choose a color other than white.");
    assert!(node.tapped);
    let choice = node.choice.expect("trailing choice");
    assert_eq!(
        choice.domain,
        EntryChoiceDomain::Color {
            excluded: vec![CardColor::White]
        }
    );
    assert_eq!(choice.domain.binding(), CharacteristicChoiceBinding::Color);
}

#[test]
fn as_enters_choices_are_one_typed_domain() {
    for (text, domain) in [
        (
            "As this artifact enters, choose a color.",
            EntryChoiceDomain::Color {
                excluded: Vec::new(),
            },
        ),
        (
            "As this enchantment enters, choose a creature type.",
            EntryChoiceDomain::CreatureType,
        ),
        (
            "As this enters, choose a basic land type.",
            EntryChoiceDomain::BasicLandType,
        ),
        (
            "As this artifact enters, choose a card type.",
            EntryChoiceDomain::CardType,
        ),
    ] {
        let node = matched(text);
        assert_eq!(node.choice.expect(text).domain, domain, "{text}");
        assert!(!node.tapped);
    }
}

#[test]
fn entry_copy_reads_the_shared_query_and_exceptions() {
    let node =
        matched("You may have this creature enter as a copy of any creature on the battlefield.");
    assert!(node.optional);
    let copy = node.copy.expect("copy");
    assert_eq!(
        copy.object.predicate,
        ObjectPredicate::CardType(ObjectCardType::Creature)
    );
    assert!(!copy.exceptions.not_legendary);

    let node =
        matched("You may have this land enter tapped as a copy of any land on the battlefield.");
    assert!(node.optional && node.tapped && node.copy.is_some());

    let node = matched(
        "You may have this creature enter as a copy of a creature you control, except it has haste.",
    );
    let copy = node.copy.expect("copy with exception");
    assert_eq!(
        copy.object.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert!(copy.exceptions.changes.is_some());
}

#[test]
fn reveal_or_tapped_reads_the_card_query_and_the_fallback() {
    let node = matched(
        "As this land enters, you may reveal a Plains or Island card from your hand. If you don't, this land enters tapped.",
    );
    assert!(!node.tapped);
    assert!(node.reveal.is_some());
}

#[test]
fn unsupported_entry_elements_fail_after_the_frame_committed() {
    for (text, state) in [
        (
            "As this creature enters, choose an opponent.",
            "replacement.entry-choice.domain",
        ),
        (
            "This creature enters tapped and doesn't untap during your untap step.",
            "replacement.entry-frame.state.coordination",
        ),
        (
            "This enchantment enters under the control of an opponent of your choice.",
            "replacement.entry-frame.state.coordination",
        ),
        (
            "As this land enters, you may reveal a Plains card from your hand. If you don't, this land enters untapped.",
            "replacement.entry-reveal.fallback-state",
        ),
    ] {
        match read(text) {
            ChildOutcome::Committed(failure) => assert_eq!(failure.state, state, "{text}"),
            _ => panic!("expected a committed failure: {text}"),
        }
    }
}

#[test]
fn other_frames_are_left_to_their_readers() {
    for text in [
        "This creature gets +1/+1.",
        "As long as you control an artifact, this creature has flying.",
        "Choose a color. Creatures of that color get +1/+1 until end of turn.",
        "You may play an additional land on each of your turns.",
    ] {
        assert!(matches!(read(text), ChildOutcome::NoMatch(_)), "{text}");
    }
}
