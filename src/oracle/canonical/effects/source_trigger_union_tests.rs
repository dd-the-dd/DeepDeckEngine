use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::recognize_entities;

fn program(text: &str, face_type: &str) -> SourceTriggerProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_promoted_source_trigger_program(&AbilityInput {
        face_name: "Synthetic union source",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn union_event(first: &str, second: &str) -> serde_json::Value {
    json!({"kind":"oneOf","events":[
        {"kind":first,"object":{"kind":"self"}},
        {"kind":second,"object":{"kind":"self"}}
    ]})
}

#[test]
fn enters_or_dies_lowers_to_a_one_of_over_the_source() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) =
        program("When this creature enters or dies, surveil 1.", "Creature")
    else {
        panic!("source transition union must parse")
    };
    assert_eq!(
        draft.rule["event"],
        union_event("enterBattlefield", "permanentDied")
    );
    assert_eq!(draft.rule["effects"][0]["kind"], "lookAtTopCards");
}

#[test]
fn union_is_order_preserving_and_composes_with_other_actions() {
    for (text, event) in [
        (
            "When this creature dies or enters the battlefield, draw a card and gain 2 life.",
            union_event("permanentDied", "enterBattlefield"),
        ),
        (
            "Whenever this artifact enters the battlefield or dies, draw a card.",
            union_event("enterBattlefield", "permanentDied"),
        ),
        (
            "When this creature enters or dies, draw a card and gain 2 life.",
            union_event("enterBattlefield", "permanentDied"),
        ),
    ] {
        let SourceTriggerProgramParseAttempt::Parsed(draft) = program(text, "Artifact Creature")
        else {
            panic!("must parse: {text}")
        };
        assert_eq!(draft.rule["event"], event, "{text}");
    }
}

#[test]
fn unproved_unions_remain_explicit_failures() {
    for text in [
        "When this creature enters or attacks, draw a card.",
        "When this creature enters or enters, draw a card.",
        "When this creature dies or dies, draw a card.",
        "When this creature enters or dies or attacks, draw a card.",
        "Whenever this land becomes tapped or dies, draw a card.",
    ] {
        assert!(
            matches!(
                program(text, "Creature"),
                SourceTriggerProgramParseAttempt::Unpromoted(_)
            ),
            "{text}"
        );
    }
}
