//! Lowering of player keyword-action events, state triggers, the
//! first-occurrence modifier and the per-player attack batch through the
//! one trigger envelope and the one event lowering.
use super::*;
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

fn lowered(text: &str) -> Result<Value, MachineFailure> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("player-event-lowering-test", &tokens);
    let envelope = match try_recognize_generic_trigger_envelope(&mut cursor, &[], None) {
        GenericTriggerEnvelopeAttempt::Recognized(envelope) => envelope,
        GenericTriggerEnvelopeAttempt::FailedCommitted(failure)
        | GenericTriggerEnvelopeAttempt::RejectedCandidate(failure) => return Err(failure),
        GenericTriggerEnvelopeAttempt::NotClaimed => panic!("{text}: not claimed"),
    };
    lower_generic_trigger_event(&mut cursor, &envelope.event)
}

fn event(text: &str) -> Value {
    lowered(text).unwrap_or_else(|failure| panic!("{text}: {failure}"))
}

fn failure(text: &str) -> MachineFailure {
    lowered(text).expect_err(text)
}

fn opponents() -> Value {
    json!({"kind": "opponentsOf", "player": controller()})
}

#[test]
fn keyword_action_events_lower_to_their_runtime_observers() {
    for (text, expected) in [
        // Lazav, Familiar Stranger; Patrolling Peacemaker.
        (
            "Whenever you commit a crime, draw a card.",
            json!({"kind": "crimeCommitted", "player": controller()}),
        ),
        (
            "Whenever an opponent commits a crime, proliferate.",
            json!({"kind": "crimeCommitted", "player": opponents()}),
        ),
        // Chittering Doom, Night Shift of the Living Dead, Brazen Dwarf.
        (
            "Whenever you roll a 4 or higher on a die, draw a card.",
            json!({"kind": "dieRolled", "player": controller(), "minimum": 4}),
        ),
        (
            "Whenever you roll a 6, draw a card.",
            json!({"kind": "dieRolled", "player": controller(), "minimum": 6, "maximum": 6}),
        ),
        (
            "Whenever you roll one or more dice, draw a card.",
            json!({"kind": "dieRolled", "player": controller(), "aggregation": "oneOrMore"}),
        ),
        // Setzer, Wandering Gambler; Karplusan Minotaur.
        (
            "Whenever you win a coin flip, draw a card.",
            json!({"kind": "coinFlipped", "player": controller(), "outcome": "won"}),
        ),
        (
            "Whenever you lose a coin flip, draw a card.",
            json!({"kind": "coinFlipped", "player": controller(), "outcome": "lost"}),
        ),
        // Sylvan Echoes.
        (
            "Whenever you clash and win, draw a card.",
            json!({"kind": "clashed", "player": controller(), "won": true}),
        ),
        // Ezuri, Stalker of Spheres.
        (
            "Whenever you proliferate, draw a card.",
            json!({"kind": "proliferated", "player": controller()}),
        ),
        // Varis, Silverymoon Ranger.
        (
            "Whenever you complete a dungeon, draw a card.",
            json!({"kind": "dungeonCompleted", "player": controller()}),
        ),
    ] {
        assert_eq!(event(text), expected, "{text}");
    }
    // Trueheart Twins: the exerted permanent's predicate.
    let exerted = event("Whenever you exert a creature, draw a card.");
    assert_eq!(exerted["kind"], "permanentExerted");
    assert_eq!(exerted["player"], controller());
    assert!(exerted["where"].is_object());
    // A never-written combination: an opponent's roll, no new code.
    assert_eq!(
        event("Whenever an opponent rolls a 5 or higher, draw a card."),
        json!({"kind": "dieRolled", "player": opponents(), "minimum": 5})
    );
}

#[test]
fn keyword_action_events_without_a_matcher_are_engine_gaps() {
    for text in [
        // The dungeon observer matches the source controller only.
        "Whenever an opponent completes a dungeon, draw a card.",
        // The first-occurrence history keys the source controller only.
        "Whenever an opponent gains life for the first time each turn, draw a card.",
        "Whenever you draw a card for the first time each turn, draw a card.",
    ] {
        assert_eq!(failure(text).state, "engine.gap.event-matcher", "{text}");
    }
}

#[test]
fn the_first_time_each_turn_reads_the_turn_history_of_life_events() {
    // Attended Healer, Vengeful Warchief.
    assert_eq!(
        event("Whenever you gain life for the first time each turn, draw a card."),
        json!({"kind": "lifeGained", "player": controller(), "firstTimeEachTurn": true})
    );
    assert_eq!(
        event("Whenever you lose life for the first time each turn, draw a card."),
        json!({"kind": "lifeLost", "player": controller(), "firstTimeEachTurn": true})
    );
}

#[test]
fn a_state_trigger_lowers_to_the_runtime_state_condition() {
    // Serendib Djinn; Immortal Coil composes another atom with no new code.
    for text in [
        "When you control no Islands, sacrifice this creature.",
        "When there are no cards in your graveyard, you lose the game.",
        // Hidden Predators: a quantified player population.
        "When an opponent controls a creature with power 4 or greater, draw a card.",
    ] {
        let lowered = event(text);
        assert_eq!(lowered["kind"], "stateConditionMet", "{text}");
        assert!(lowered["condition"].is_object(), "{text}");
    }
}

#[test]
fn an_attack_batch_with_an_attacked_player_is_one_event_per_player() {
    // Firkraag, Cunning Instigator.
    let firkraag =
        event("Whenever one or more Dragons you control attack an opponent, draw a card.");
    assert_eq!(firkraag["kind"], "attackersAttackedPlayer");
    assert_eq!(firkraag["attacked"], opponents());
    assert_eq!(firkraag["attackers"]["controller"], controller());
    assert!(firkraag["attackers"]["where"].is_object());
    // Reveille Squad: attackers of any controller attacking you.
    let squad = event("Whenever one or more creatures attack you, draw a card.");
    assert_eq!(squad["attacked"], controller());
    assert!(squad["attackers"].get("controller").is_none());
    // Horn of the Mark: a larger batch.
    assert_eq!(
        event("Whenever two or more creatures you control attack a player, draw a card.")["attackers"]
            ["minimum"],
        2
    );
    // A permanent among the attacked alternatives has no per-player matcher.
    assert_eq!(
        failure(
            "Whenever one or more creatures you control attack you or a planeswalker you control, draw a card."
        )
        .state,
        "engine.gap.event-matcher"
    );
}

#[test]
fn an_opponents_permanent_tapped_for_mana_keeps_the_written_relation() {
    // Sanctimony, War's Toll, Vorinclex.
    let lowered = event("Whenever an opponent taps a Mountain for mana, draw a card.");
    assert_eq!(lowered["kind"], "controlledPermanentManaAbilityActivated");
    assert_eq!(lowered["player"], opponents());
}

#[test]
fn a_typed_attached_mana_trigger_is_folded_into_the_mana_ability() {
    let attached = json!({"kind": "attachedPermanent", "attachment": self_ref()});
    let effect = |player: Value| json!({"kind": "addMana", "player": player, "mana": "{G}"});
    assert!(attached_mana_trigger_is_folded(&[effect(
        json!({"kind": "controllerOf", "object": attached})
    )]));
    // `you add`: the Aura's controller is not the land's controller.
    assert!(!attached_mana_trigger_is_folded(&[effect(controller())]));
    // Any other attribute of the action is not folded.
    let mut restricted = effect(json!({"kind": "controllerOf", "object": attached}));
    restricted["spendRestriction"] = json!({"kind": "creatureSpells"});
    assert!(!attached_mana_trigger_is_folded(&[restricted]));
}
