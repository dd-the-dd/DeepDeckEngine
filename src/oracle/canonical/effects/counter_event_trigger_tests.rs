//! Counter placement events lowered to the runtime `countersPlaced` observer.
use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn program(text: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic counter event",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn event(text: &str) -> Value {
    match program(text) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule["event"].clone(),
        GenericTriggerProgramAttempt::FailedCommitted(failure)
        | GenericTriggerProgramAttempt::RejectedCandidate(failure) => panic!("{text}: {failure}"),
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not applicable"),
    }
}

fn committed_state(text: &str) -> &'static str {
    match program(text) {
        GenericTriggerProgramAttempt::FailedCommitted(failure) => failure.state,
        GenericTriggerProgramAttempt::Parsed(_) => panic!("{text}: unexpectedly parsed"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure) => {
            panic!("{text}: claimed trigger was only rejected: {failure}")
        }
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not claimed"),
    }
}

#[test]
fn counter_events_compose_kind_recipient_and_placer() {
    let creature =
        lower_permanent_object_predicate(&ObjectPredicate::CardType(ObjectCardType::Creature));
    // Herd Baloth: the source as recipient.
    assert_eq!(
        event(
            "Whenever one or more +1/+1 counters are put on this creature, you may create a 4/4 green Beast creature token."
        ),
        json!({"kind": "countersPlaced", "player": controller(), "counter": "+1/+1", "object": self_ref()})
    );
    // Enduring Scalelord: another permanent the controller controls.
    assert_eq!(
        event(
            "Whenever one or more +1/+1 counters are put on another creature you control, draw a card."
        ),
        json!({"kind": "countersPlaced", "player": controller(), "counter": "+1/+1",
            "where": creature, "excludeSource": true})
    );
    // Wickersmith's Tools: any controller.
    assert_eq!(
        event("Whenever one or more -1/-1 counters are put on a creature, draw a card."),
        json!({"kind": "countersPlaced", "player": controller(), "counter": "-1/-1",
            "where": creature, "anyController": true})
    );
    // Unused combination: the controller as placer of another counter kind.
    assert_eq!(
        event(
            "Whenever you put one or more -1/-1 counters on a creature you control, you gain 1 life."
        ),
        json!({"kind": "countersPlaced", "player": controller(), "counter": "-1/-1",
            "where": creature, "placedBy": controller()})
    );
}

#[test]
fn counter_event_near_misses_fail_inside_the_trigger_frame() {
    assert_eq!(
        committed_state("Whenever one or more counters are put on this creature, draw a card."),
        "trigger.counters.kind-support"
    );
    assert_eq!(
        committed_state(
            "Whenever an opponent puts one or more +1/+1 counters on a creature, draw a card."
        ),
        "trigger.counters.placer-support"
    );
    assert_eq!(
        committed_state(
            "Whenever one or more +1/+1 counters are put on a creature an opponent controls, draw a card."
        ),
        "trigger.counters.recipient-support"
    );
}
