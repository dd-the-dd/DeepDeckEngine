//! `another` / `other` in an intervening `if` (CR 603.4): the count excludes
//! the object the ability is about. When that object is the ability's own
//! source, the lowered count excludes the source (`excludeSource`), which
//! the runtime authenticates by incarnation; when the event binds another
//! object, the exclusion is refused with its own diagnostic.
use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn attempt(text: &str, face_type_line: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic guarded trigger",
        face_type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn guard(text: &str) -> Value {
    match attempt(text, "Creature — Human Soldier") {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule["condition"].clone(),
        GenericTriggerProgramAttempt::NotApplicable => panic!("not applicable: {text}"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure)
        | GenericTriggerProgramAttempt::FailedCommitted(failure) => {
            panic!("{text}: {} expected {}", failure.state, failure.expected)
        }
    }
}

/// The controlled-permanent count of a `compare` guard.
fn counted(condition: &Value) -> &Value {
    assert_eq!(condition["kind"], "compare", "{condition}");
    &condition["left"]
}

#[test]
fn another_in_a_source_entry_guard_excludes_the_source() {
    // Resistance Squad.
    let condition = guard("When this creature enters, if you control another Human, draw a card.");
    assert_eq!(condition["operator"], ">=");
    assert_eq!(condition["right"], json!({"kind":"integer","value":1}));
    assert_eq!(counted(&condition)["kind"], "countPermanents");
    assert_eq!(counted(&condition)["excludeSource"], json!(true));
}

#[test]
fn other_counts_compose_with_thresholds_and_events_without_an_object() {
    // Platoon Dispenser: a step event binds no object; `two or more other`
    // is the same exclusion with another threshold.
    let condition = guard(
        "At the beginning of your end step, if you control two or more other creatures, draw a card.",
    );
    assert_eq!(condition["right"], json!({"kind":"integer","value":2}));
    assert_eq!(counted(&condition)["excludeSource"], json!(true));
    // Unused combination: a source attack event with a characteristic
    // relative clause on the other permanent (Nessian Hornbeetle's
    // condition on Iron Man's event).
    let condition = guard(
        "Whenever this creature attacks, if you control another creature with power 4 or greater, draw a card.",
    );
    assert_eq!(counted(&condition)["excludeSource"], json!(true));
    // `no other` is the zero comparison of the same count (Dust Stalker).
    let condition =
        guard("At the beginning of each end step, if you control no other creatures, draw a card.");
    assert_eq!(counted(&condition)["excludeSource"], json!(true));
}

#[test]
fn other_naming_the_event_object_stays_an_explicit_failure() {
    // Valakut, the Molten Pinnacle: `other` excludes the entering Mountain.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) = attempt(
        "Whenever a Mountain you control enters, if you control at least five other Mountains, draw a card.",
        "Land",
    ) else {
        panic!("the event-object exclusion must stay an owned failure")
    };
    assert_eq!(failure.state, "trigger.program.guard-other-event-object");
}
