//! The shared binding environment: real cards, an unused recombination and
//! near misses for each reference it resolves. Runtime proofs live in
//! `engine/tests/binding_environment_runtime.rs`.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

/// Every `dealDamage` recipient of the card's canonical rules.
fn damage_recipients(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .filter(|effect| effect["kind"] == "dealDamage")
        .map(|effect| effect["recipient"].clone())
        .collect()
}

const ENCHANTMENT: &str = "Enchantment";
const CREATURE: &str = "Creature \u{2014} Goblin";
const INSTANT: &str = "Instant";

#[test]
fn that_player_names_the_player_the_event_records() {
    let triggering = json!({"kind":"triggeringPlayer"});
    // Real cards: Megrim (discard), Underworld Dreams (draw), Eidolon of the
    // Great Revel (cast). Unused recombination: a draw trigger whose damage
    // recipient is followed by a gapped second damage predicate.
    for oracle in [
        "Whenever an opponent discards a card, this enchantment deals 2 damage to that player.",
        "Whenever an opponent draws a card, this enchantment deals 1 damage to that player.",
        "Whenever a player casts a spell with mana value 3 or less, this enchantment deals 2 damage to that player.",
        "Whenever an opponent draws a card, this enchantment deals 1 damage to that player and 1 damage to you.",
    ] {
        let recipients = damage_recipients(ENCHANTMENT, oracle);
        assert_eq!(recipients.first(), Some(&triggering), "{oracle}");
    }
}

#[test]
fn that_player_prefers_the_player_declared_earlier_in_the_program() {
    // `that player` after `target player` is the declared target, never the
    // event player.
    let recipients = damage_recipients(
        INSTANT,
        "Target player discards a card. This spell deals 2 damage to that player.",
    );
    assert_eq!(recipients[0]["kind"], "chosenTarget", "{recipients:?}");
}

#[test]
fn possessive_player_relation_reads_the_declared_target() {
    // Real cards: Consign to the Pit, Ionize; unused recombination: an
    // activated tap followed by damage to the tapped creature's owner.
    for (type_line, oracle, relation) in [
        (
            INSTANT,
            "Destroy target creature. This spell deals 2 damage to that creature's controller.",
            "controllerOf",
        ),
        (
            CREATURE,
            "{T}: Tap target creature. This creature deals 1 damage to that creature's owner.",
            "ownerOf",
        ),
    ] {
        let recipients = damage_recipients(type_line, oracle);
        let recipient = recipients.last().expect("a damage recipient");
        assert_eq!(recipient["kind"], relation, "{oracle}: {recipients:?}");
        assert_eq!(recipient["object"]["kind"], "chosenTarget", "{oracle}");
    }
}

#[test]
fn object_references_name_the_bound_object() {
    // Real cards: Abyssal Hunter (declared target), Ornery Goblin (the other
    // combatant of a block event).
    let recipients = damage_recipients(
        CREATURE,
        "{B}, {T}: Tap target creature. This creature deals damage equal to its power to that creature.",
    );
    assert_eq!(recipients[0]["kind"], "chosenTarget", "{recipients:?}");
    let recipients = damage_recipients(
        CREATURE,
        "Whenever this creature blocks or becomes blocked by a creature, this creature deals 1 damage to that creature.",
    );
    assert_eq!(recipients[0], json!({"kind":"combatCounterpart"}));
}

#[test]
fn references_without_a_binding_fail_explicitly() {
    for (type_line, oracle, state) in [
        // No event records a player and no player was declared.
        (
            CREATURE,
            "{T}: This creature deals 1 damage to that player.",
            "player-subject.bound.resolve",
        ),
        // The nominal does not follow from the declared selector.
        (
            INSTANT,
            "Destroy target artifact. This spell deals 2 damage to that creature's controller.",
            "player-subject.object-relation.antecedent",
        ),
        // `defending player` outside an attack trigger.
        (
            CREATURE,
            "{T}: This creature deals 1 damage to defending player.",
            "player-subject.defending-player",
        ),
    ] {
        let (status, _, states) = parse(type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}");
        assert!(
            states.iter().any(|candidate| candidate == state),
            "{oracle}: {states:?}"
        );
    }
}
