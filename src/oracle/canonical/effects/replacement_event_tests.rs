use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};

fn attempt(type_line: &str, text: &str) -> ReplacementFrameAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Test Card",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    try_parse_event_replacement(&input)
}

#[test]
fn a_replaced_draw_is_the_shared_draw_count_modifier() {
    let ReplacementFrameAttempt::Parsed(draft) = attempt(
        "Artifact",
        "If you would draw a card except the first one you draw in each of your draw steps, draw two cards instead.",
    ) else {
        panic!("draw replacement");
    };
    assert_eq!(draft.rule["kind"], "staticAbility");
    assert_eq!(
        draft.rule["modifiers"][0],
        json!({
            "kind": "increaseEachDrawCount",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": 1,
            "exceptFirstInDrawStep": true,
        })
    );
}

#[test]
fn a_replaced_draw_needs_a_battlefield_source() {
    let ReplacementFrameAttempt::Committed(failure) = attempt(
        "Sorcery",
        "If you would draw a card, draw two cards instead.",
    ) else {
        panic!("a spell has no battlefield-active replacement");
    };
    assert_eq!(failure.state, "replacement.event.validation");
}
