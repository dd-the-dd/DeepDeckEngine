use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_entities},
};

fn rule(text: &str) -> Result<Value, MachineFailure> {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    match try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic dealt-damage trigger",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    }) {
        GenericTriggerProgramAttempt::Parsed(draft) => Ok(draft.rule),
        GenericTriggerProgramAttempt::FailedCommitted(failure)
        | GenericTriggerProgramAttempt::RejectedCandidate(failure) => Err(failure),
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not applicable"),
    }
}

fn event(text: &str) -> Value {
    rule(text).unwrap_or_else(|failure| panic!("{text}: {failure}"))["event"].clone()
}

fn failure_state(text: &str) -> &'static str {
    rule(text).expect_err(text).state
}

#[test]
fn source_dealt_damage_exports_its_amount_to_the_body() {
    // Printed text of a granted ability and of an enrage trigger.
    let parsed =
        rule("Whenever this creature is dealt damage, put that many +1/+1 counters on it.")
            .unwrap();
    assert_eq!(
        parsed["event"],
        json!({"kind": "permanentDealtDamage", "object": self_ref()})
    );
    assert_eq!(
        parsed["effects"][0]["count"],
        json!({"kind": "decisionResult", "decisionId": "damageAmount"})
    );
}

#[test]
fn noncombat_is_a_shared_complement_of_every_received_damage_subject() {
    assert_eq!(
        event("Whenever this creature is dealt noncombat damage, draw a card."),
        json!({"kind": "permanentDealtDamage", "object": self_ref(), "noncombatOnly": true})
    );
    assert_eq!(
        event("Whenever a creature you control is dealt noncombat damage, draw a card."),
        json!({
            "kind": "controlledPermanentDealtDamage",
            "player": controller(),
            "where": {"kind": "cardTypeContains", "value": "Creature"},
            "noncombatOnly": true,
        })
    );
    let opponents = json!({"kind": "opponentsOf", "player": controller()});
    assert_eq!(
        event("Whenever an opponent is dealt noncombat damage, draw a card."),
        json!({
            "kind": "opponentDealtDamage",
            "player": opponents,
            "aggregation": "perRecipient",
            "noncombatOnly": true,
        })
    );
    assert_eq!(
        event("Whenever one or more opponents are dealt noncombat damage, draw a card.")["aggregation"],
        json!("oneOrMore")
    );
}

#[test]
fn combat_is_a_shared_complement_of_every_received_damage_subject() {
    assert_eq!(
        event("Whenever a creature you control is dealt combat damage, draw a card."),
        json!({
            "kind": "controlledPermanentDealtDamage",
            "player": controller(),
            "where": {"kind": "cardTypeContains", "value": "Creature"},
            "combatOnly": true,
        })
    );
    assert_eq!(
        event("Whenever an opponent is dealt combat damage, draw a card.")["combatOnly"],
        json!(true)
    );
}

#[test]
fn an_unseen_combination_reuses_the_event_quantity_in_every_action() {
    let parsed = rule(
        "Whenever a creature you control is dealt damage, you draw that many cards and lose that much life.",
    )
    .unwrap();
    let amount = json!({"kind": "decisionResult", "decisionId": "damageAmount"});
    assert_eq!(parsed["effects"][0]["count"], amount);
    assert_eq!(parsed["effects"][1]["amount"], amount);
}

#[test]
fn unrepresentable_received_damage_events_fail_at_their_own_state() {
    for (text, state) in [
        (
            "Whenever another creature you control is dealt damage, draw a card.",
            "trigger.object-event.source-exclusion-support",
        ),
        (
            "Whenever one or more creatures you control are dealt damage, draw a card.",
            "trigger.object-event.verb-support",
        ),
        (
            "Whenever a creature is dealt damage, draw a card.",
            "trigger.object-event.verb-support",
        ),
        (
            "Whenever you are dealt damage, draw a card.",
            "trigger.program.player-event-support",
        ),
        (
            "Whenever a player is dealt damage, draw a card.",
            "trigger.program.player-event-support",
        ),
        (
            "Whenever one or more players gain life, draw a card.",
            "trigger.program.player-event-support",
        ),
        // A trailing qualifier is not swallowed by the damage event.
        (
            "Whenever an opponent is dealt combat damage during your turn, draw a card.",
            "trigger.envelope.separator",
        ),
    ] {
        assert_eq!(failure_state(text), state, "{text}");
    }
}

#[test]
fn that_many_needs_an_event_that_exports_a_quantity() {
    for text in [
        "Whenever this creature attacks, draw that many cards.",
        "Whenever this creature enters, put that many +1/+1 counters on it.",
    ] {
        assert!(rule(text).is_err(), "{text}");
    }
}
