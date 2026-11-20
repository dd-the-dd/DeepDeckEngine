//! Unit tests for the `OBJECTS VERBed this way` child; card-level proofs live
//! in the canonical result-reuse tests.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn read(text: &str) -> MachineResult<ActionResultReferenceNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("action-result-test", &tokens);
    let node = recognize_action_result_reference(&mut cursor)?;
    cursor.finish()?;
    Ok(node)
}

#[test]
fn restated_noun_and_verb_are_typed() {
    let node = read("creature destroyed this way").expect("destroyed result");
    assert_eq!(node.verb, ActionResultVerb::Destroyed);
    let node = read("creature card milled this way").expect("milled result");
    assert_eq!(node.verb, ActionResultVerb::Milled);
    assert!(node.predicate.has_domain(ObjectKind::Card));
}

#[test]
fn other_verbs_and_controlled_nouns_are_refused() {
    assert!(!action_result_reference_starts_text(
        "creature returned this way"
    ));
    assert!(read("creature you control destroyed this way").is_err());
}

fn action_result_reference_starts_text(text: &str) -> bool {
    let tokens = recognize_entities(text);
    let cursor = OracleTokenCursor::new("action-result-test", &tokens);
    action_result_reference_starts(&cursor)
}
