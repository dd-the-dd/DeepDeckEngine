use super::*;
use crate::oracle::syntax::recognize_entities;

fn union(text: &str) -> (MachineResult<SourceEventUnionNode>, Option<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("source-event-union-test", &tokens);
    cursor.consume_type("triggerWord").expect("trigger word");
    let node = recognize_source_event_union(&mut cursor, &[], None);
    (node, cursor.peek().map(str::to_string))
}

#[test]
fn source_verbs_compose_into_an_ordered_union_and_stop_at_the_envelope_comma() {
    use SourceEventVerb::*;
    for (text, verbs) in [
        // Spellgorger Barbarian, Spine of Ish Sah, Dread Specter's shape.
        (
            "When this creature leaves the battlefield, draw a card.",
            vec![LeftBattlefield],
        ),
        (
            "When this artifact is put into a graveyard from the battlefield, draw a card.",
            vec![PutIntoGraveyardFromBattlefield],
        ),
        (
            "Whenever this creature attacks or blocks, draw a card.",
            vec![Attacked, Blocked],
        ),
        // Unused combination of the same atoms: entry, departure and tapping.
        (
            "Whenever this enchantment enters or leaves the battlefield, draw a card.",
            vec![Entered, LeftBattlefield],
        ),
        (
            "Whenever this land becomes tapped or becomes untapped, draw a card.",
            vec![BecameTapped, BecameUntapped],
        ),
    ] {
        let (node, next) = union(text);
        let node = node.unwrap_or_else(|failure| panic!("{text}: {failure}"));
        assert_eq!(node.source, LocalObjectReference::SourceSelf, "{text}");
        assert_eq!(node.verbs, verbs, "{text}");
        assert_eq!(next.as_deref(), Some(","), "{text}");
    }
}

#[test]
fn source_event_near_misses_fail_at_their_own_atom() {
    for (text, state) in [
        // An owner relation on the destination has no event matcher.
        (
            "When this creature is put into your graveyard from the battlefield, draw a card.",
            "event.source.put-into-graveyard.origin",
        ),
        (
            "Whenever this creature becomes enchanted, draw a card.",
            "event.source.becomes",
        ),
        (
            "Whenever this creature attacks or attacks, draw a card.",
            "event.source.union-distinct",
        ),
        (
            "Whenever a creature attacks, draw a card.",
            "reference.source.start",
        ),
    ] {
        let (node, _) = union(text);
        let failure = node.expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
}
