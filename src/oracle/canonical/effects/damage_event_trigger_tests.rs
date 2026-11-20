//! Dealt-damage events: the source, its attached object or a permanent query
//! deals [combat|noncombat] damage to a player or permanent recipient.
use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn program(text: &str, face_type: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic damage event",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn event(text: &str, face_type: &str) -> Value {
    match program(text, face_type) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule["event"].clone(),
        GenericTriggerProgramAttempt::FailedCommitted(failure)
        | GenericTriggerProgramAttempt::RejectedCandidate(failure) => panic!("{text}: {failure}"),
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not applicable"),
    }
}

fn committed_state(text: &str) -> &'static str {
    match program(text, "Creature") {
        GenericTriggerProgramAttempt::FailedCommitted(failure) => failure.state,
        GenericTriggerProgramAttempt::Parsed(_) => panic!("{text}: unexpectedly parsed"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure) => {
            panic!("{text}: claimed trigger was only rejected: {failure}")
        }
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not claimed"),
    }
}

fn creature() -> Value {
    lower_permanent_object_predicate(&ObjectPredicate::CardType(ObjectCardType::Creature))
}

#[test]
fn the_source_deals_damage_to_a_typed_recipient() {
    let opponents = json!({"kind": "opponentsOf", "player": controller()});
    // Hypnotic Specter: `that player` names the damaged opponent.
    assert_eq!(
        event(
            "Whenever this creature deals damage to an opponent, that player discards a card at random.",
            "Creature"
        ),
        json!({"kind": "damageDealtBySource", "source": {"kind": "self"},
            "recipient": {"kind": "player", "player": opponents}})
    );
    // Pit Scorpion: any player.
    assert_eq!(
        event(
            "Whenever this creature deals damage to a player, that player loses 1 life.",
            "Creature"
        )["recipient"],
        json!({"kind": "player", "player": {"kind": "eachPlayer"}})
    );
    // Combat damage to a creature: a permanent recipient.
    assert_eq!(
        event(
            "Whenever this creature deals combat damage to a creature, you gain 2 life.",
            "Creature"
        ),
        json!({"kind": "damageDealtBySource", "source": {"kind": "self"}, "combatOnly": true,
            "recipient": {"kind": "permanent", "where": creature()}})
    );
    // The established combat-damage-to-a-player atom is unchanged.
    assert_eq!(
        event(
            "Whenever this creature deals combat damage to a player, draw a card.",
            "Creature"
        ),
        json!({"kind": "combatDamageToPlayer", "source": self_ref()})
    );
}

#[test]
fn attached_and_queried_subjects_share_the_damage_complement() {
    // Giant's Skewer.
    assert_eq!(
        event(
            "Whenever equipped creature deals combat damage to a creature, you gain 1 life.",
            "Artifact — Equipment"
        ),
        json!({"kind": "damageDealtBySource",
            "source": {"kind": "attachedPermanent", "attachment": self_ref()},
            "combatOnly": true, "recipient": {"kind": "permanent", "where": creature()}})
    );
    // Reconnaissance Mission: one trigger per creature that deals the damage.
    assert_eq!(
        event(
            "Whenever a creature you control deals combat damage to a player, you may draw a card.",
            "Enchantment"
        ),
        json!({"kind": "damageDealtBySource",
            "source": {"kind": "permanents", "controller": controller(), "where": creature()},
            "combatOnly": true,
            "recipient": {"kind": "player", "player": {"kind": "eachPlayer"}}})
    );
    // Unused combination: an enchanted creature's noncombat damage to you.
    assert_eq!(
        event(
            "Whenever enchanted creature deals noncombat damage to you, draw a card.",
            "Enchantment — Aura"
        ),
        json!({"kind": "damageDealtBySource",
            "source": {"kind": "attachedPermanent", "attachment": self_ref()},
            "noncombatOnly": true, "recipient": {"kind": "player", "player": controller()}})
    );
    // The plural established form keeps its grouped matcher.
    assert_eq!(
        event(
            "Whenever one or more creatures you control deal combat damage to a player, draw a card.",
            "Enchantment"
        )["kind"],
        "controlledCreaturesCombatDamageToPlayer"
    );
}

#[test]
fn received_combat_damage_filters_the_damage_kind() {
    // Wall of Essence.
    assert_eq!(
        event(
            "Whenever this creature is dealt combat damage, you gain that much life.",
            "Creature"
        ),
        json!({"kind": "permanentDealtDamage", "object": self_ref(), "combatOnly": true})
    );
}

#[test]
fn dealt_damage_near_misses_fail_inside_the_trigger_frame() {
    // No recipient: one event may damage several recipients at once.
    assert_eq!(
        committed_state("Whenever this creature deals damage, you gain that much life."),
        "trigger.dealt-damage.recipient-support"
    );
    // A one-or-more batch of damage sources to a permanent recipient.
    assert_eq!(
        committed_state(
            "Whenever one or more creatures you control deal combat damage to a creature, draw a card."
        ),
        "trigger.dealt-damage.source-support"
    );
}
