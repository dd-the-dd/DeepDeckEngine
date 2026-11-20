//! Player-action events: one shared actor composed with discard, cycling and
//! completed named keyword actions, lowered to their runtime observers.
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
        face_name: "Synthetic player action",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn rule(text: &str) -> Value {
    match program(text) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule,
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

fn opponents() -> Value {
    json!({"kind": "opponentsOf", "player": controller()})
}

#[test]
fn discard_events_compose_actor_card_query_and_triggering_player() {
    // Liliana's Caress: an opponent actor; `that player` is the discarding player.
    let caress = rule("Whenever an opponent discards a card, that player loses 2 life.");
    assert_eq!(
        caress["event"],
        json!({"kind": "playerDiscardedCard", "player": opponents()})
    );
    // Lady Spider: the controller actor.
    assert_eq!(
        rule("Whenever you discard a card, put a +1/+1 counter on target Spider you control.")["event"],
        json!({"kind": "playerDiscardedCard", "player": controller()})
    );
    // Hashaton: the shared card query filters the discarded card.
    let creature = rule("Whenever you discard a creature card, you gain 1 life.");
    assert_eq!(creature["event"]["kind"], "playerDiscardedCard");
    assert!(creature["event"]["where"].is_object());
    // Unused combination: any player with the same card query and binding.
    let any = rule("Whenever a player discards a land card, that player loses 1 life.");
    assert_eq!(any["event"]["player"], json!({"kind": "eachPlayer"}));
    assert!(any["event"]["where"].is_object());
    assert_ne!(any["event"]["where"], creature["event"]["where"]);
}

#[test]
fn cycling_is_a_discard_so_cycle_or_discard_observes_one_event() {
    // Archfiend of Ifnir (CR 702.29a): the union is the discard event alone.
    assert_eq!(
        rule("Whenever you cycle or discard another card, scry 1.")["event"],
        json!({"kind": "playerDiscardedCard", "player": controller(), "excludeSource": true})
    );
    // Cycling alone is its own observer.
    assert_eq!(
        rule("Whenever you cycle a card, you gain 1 life.")["event"],
        json!({"kind": "cardCycled", "player": controller()})
    );
    // Astral Drift's second branch: `another card` excludes the source.
    assert_eq!(
        rule("Whenever you cycle another card, draw a card.")["event"],
        json!({"kind": "cardCycled", "player": controller(), "excludeSource": true})
    );
    // Unused combination: an opponent actor for cycling.
    assert_eq!(
        rule("Whenever an opponent cycles a card, you draw a card.")["event"]["player"],
        opponents()
    );
}

#[test]
fn named_keyword_actions_lower_to_their_completion_signal() {
    let scry = json!({"kind": "abilityPerformed", "abilityId": "scry", "player": controller()});
    let surveil =
        json!({"kind": "abilityPerformed", "abilityId": "surveil", "player": controller()});
    // Matoya, Archon Elder.
    assert_eq!(
        rule("Whenever you scry or surveil, draw a card.")["event"],
        json!({"kind": "oneOf", "events": [scry, surveil]})
    );
    // Chance-Met Elves: the shared once-each-turn limit still composes.
    let elves = rule(
        "Whenever you scry, put a +1/+1 counter on this creature. This ability triggers only once each turn.",
    );
    assert_eq!(elves["event"], scry);
    assert_eq!(elves["triggerLimit"]["kind"], "onceEachTurn");
    // Unused combination: an opponent actor and the triggering player.
    let opponent = rule("Whenever an opponent surveils, that player loses 1 life.");
    assert_eq!(
        opponent["event"],
        json!({"kind": "abilityPerformed", "abilityId": "surveil", "player": opponents()})
    );
}

#[test]
fn player_action_near_misses_fail_inside_the_trigger_frame() {
    // A catalog action without an inspected signal producer.
    assert_eq!(
        committed_state("Whenever you explore, draw a card."),
        "trigger.player-event.action-support"
    );
    // A plural actor has no batch observer for these actions.
    assert_eq!(
        committed_state("Whenever one or more players discard a card, draw a card."),
        "trigger.program.player-event-support"
    );
    // A numeric card characteristic needs a captured discarded-card snapshot.
    assert_eq!(
        committed_state("Whenever you discard a card with mana value 3 or greater, draw a card."),
        "trigger.player-event.card-filter-support"
    );
}
