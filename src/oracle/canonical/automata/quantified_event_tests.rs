use super::*;
use crate::oracle::syntax::recognize_entities;

fn event(text: &str) -> MachineResult<ObjectEventNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-event-test", &tokens);
    let event = recognize_object_event(&mut cursor)?;
    cursor.finish()?;
    Ok(event)
}

#[test]
fn the_verb_is_the_event_and_the_bound_is_the_subject_multiplicity() {
    for (text, verb, multiplicity) in [
        (
            "one or more cards leave your graveyard",
            ObjectEventVerb::LeaveControllerGraveyard,
            EventMultiplicity::PerEvent { minimum: 1 },
        ),
        (
            "a card leaves your graveyard",
            ObjectEventVerb::LeaveControllerGraveyard,
            EventMultiplicity::PerObject,
        ),
        (
            "one or more creatures you control with flying attack",
            ObjectEventVerb::Attack,
            EventMultiplicity::PerEvent { minimum: 1 },
        ),
        (
            "one or more Allies you control deal combat damage to a player",
            ObjectEventVerb::DealCombatDamageToPlayer,
            EventMultiplicity::PerEvent { minimum: 1 },
        ),
        // The shared numeric atom reads digits and number words alike.
        (
            "2 or more creatures you control attack",
            ObjectEventVerb::Attack,
            EventMultiplicity::PerEvent { minimum: 2 },
        ),
    ] {
        let parsed = event(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert_eq!(parsed.verb, verb, "{text}");
        assert_eq!(parsed.subject.multiplicity, multiplicity, "{text}");
    }
}

#[test]
fn other_excludes_the_source_from_a_plural_subject() {
    let parsed = event("one or more other creatures you control enter").unwrap();
    assert!(parsed.subject.exclude_source);
    assert_eq!(parsed.verb, ObjectEventVerb::Enter);
}

#[test]
fn verb_number_must_agree_with_the_quantified_subject() {
    for (text, state) in [
        (
            "one or more cards leaves your graveyard",
            "event.object.verb-agreement",
        ),
        ("a card leave your graveyard", "event.object.verb-agreement"),
        (
            "one or more cards leave your library",
            "event.object.complement",
        ),
        (
            // The shared numeric child owns the threshold word.
            "zero or more cards leave your graveyard",
            "numeric.atom.start",
        ),
    ] {
        let failure = event(text).unwrap_err();
        assert_eq!(failure.state, state, "{text}");
    }
}

#[test]
fn passive_received_damage_agrees_with_the_subject_and_reads_the_shared_complement() {
    for (text, kind, multiplicity) in [
        (
            "a creature you control is dealt damage",
            DamageEventKind::Any,
            EventMultiplicity::PerObject,
        ),
        (
            "another creature is dealt noncombat damage",
            DamageEventKind::Noncombat,
            EventMultiplicity::PerObject,
        ),
        (
            "one or more creatures you control are dealt combat damage",
            DamageEventKind::Combat,
            EventMultiplicity::PerEvent { minimum: 1 },
        ),
    ] {
        let parsed = event(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert_eq!(parsed.verb, ObjectEventVerb::BeDealtDamage(kind), "{text}");
        assert_eq!(parsed.subject.multiplicity, multiplicity, "{text}");
    }
    for (text, state) in [
        ("a creature are dealt damage", "event.object.verb-agreement"),
        (
            "one or more creatures is dealt damage",
            "event.object.verb-agreement",
        ),
        ("a creature is dealt", "event.dealt-damage.noun"),
        ("a creature is dealt life", "event.dealt-damage.noun"),
    ] {
        assert_eq!(event(text).unwrap_err().state, state, "{text}");
    }
}
