//! The shared combat clause (CR 508.1c-d, 509.1b-c) under every subject that
//! supplies it: source, attachment, battlefield collection (static) and
//! targets, collections and verb chains (temporary).
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn request(type_line: &str, text: &str) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Synthetic Restriction".into(),
        type_line: type_line.into(),
        mana_cost: Some("{1}{W}".into()),
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: vec![],
    }
}

/// The rule of the last ability (Auras carry their `Enchant` line first).
fn rule(type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card_native(request(type_line, text));
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .last()
        .and_then(|ability| ability.rule.clone())
        .expect("a canonical rule")
}

fn failure_state(type_line: &str, text: &str) -> String {
    let parsed = parse_oracle_card_native(request(type_line, text));
    assert_ne!(parsed.status, "canonical", "{text}");
    parsed
        .diagnostics
        .iter()
        .chain(
            parsed
                .abilities
                .iter()
                .flat_map(|ability| ability.diagnostics.iter()),
        )
        .find_map(|diagnostic| diagnostic.state.clone())
        .unwrap_or_default()
}

const AURA: &str = "Enchantment \u{2014} Aura";
fn enchanted() -> Value {
    json!({ "kind": "attachedPermanent", "attachment": { "kind": "self" } })
}

#[test]
fn pacifism_lowers_both_prohibitions_on_the_enchanted_creature() {
    let rule = rule(
        AURA,
        "Enchant creature\nEnchanted creature can't attack or block.",
    );
    assert_eq!(
        rule["modifiers"],
        json!([
            { "kind": "grantKeyword", "objects": enchanted(), "keyword": "cantAttack" },
            { "kind": "grantKeyword", "objects": enchanted(), "keyword": "cantBlock" },
        ])
    );
}

#[test]
fn a_combat_clause_composes_with_other_continuous_predicates_of_the_subject() {
    // Cagemail, Vow of Duty, Hellraiser Goblin: the clause is one more
    // predicate of the same subject, after `gets`, `has` or a serial comma.
    let cagemail = rule(
        "Artifact \u{2014} Equipment",
        "Equipped creature gets +2/+2 and can't attack.",
    );
    assert_eq!(cagemail["modifiers"][1]["keyword"], "cantAttack");
    let vow = rule(
        AURA,
        "Enchant creature\nEnchanted creature gets +2/+2, has vigilance, and can't attack you or planeswalkers you control.",
    );
    assert_eq!(
        vow["modifiers"][2],
        json!({
            "kind": "cantAttackSourceController", "objects": enchanted(),
            "defenders": "playerAndPlaneswalkers",
        })
    );
    let goblin = rule(
        "Creature \u{2014} Goblin",
        "Creatures you control have haste and attack each combat if able.",
    );
    assert_eq!(goblin["modifiers"][1]["keyword"], "attackEachCombatIfAble");
    assert_eq!(goblin["modifiers"][1]["objects"]["kind"], "permanents");
}

#[test]
fn collection_subjects_reuse_the_shared_object_query() {
    // Moat and Stormtide Leviathan: `without K [or K]` is the complement of
    // the shared keyword qualifier.
    let moat = rule("Enchantment", "Creatures without flying can't attack.");
    assert_eq!(
        moat["modifiers"][0]["objects"]["where"],
        json!({ "kind": "and", "operands": [
            { "kind": "cardTypeContains", "value": "Creature" },
            { "kind": "not", "operand": { "kind": "hasKeyword", "value": "flying" } },
        ]}),
        "{moat}"
    );
    let leviathan = rule(
        "Creature \u{2014} Leviathan",
        "Creatures without flying or islandwalk can't attack.",
    );
    assert_eq!(leviathan["modifiers"][0]["keyword"], "cantAttack");
    // Angler Turtle: a requirement on an opponent-controlled collection.
    let turtle = rule(
        "Creature \u{2014} Turtle",
        "Creatures your opponents control attack each combat if able.",
    );
    assert_eq!(
        turtle["modifiers"][0]["objects"]["controller"]["kind"],
        "opponentsOf"
    );
}

#[test]
fn pair_restrictions_lower_to_block_restrictions_on_the_subject() {
    // Invisibility: the complement inside the creature domain.
    let invisibility = rule(
        AURA,
        "Enchant creature\nEnchanted creature can't be blocked except by Walls.",
    );
    assert_eq!(invisibility["modifiers"][0]["kind"], "blockRestriction");
    assert_eq!(invisibility["modifiers"][0]["attackers"], enchanted());
    // Air Bladder: `can block only QUERY` restricts the subject as blocker.
    let bladder = rule(
        AURA,
        "Enchant creature\nEnchanted creature can block only creatures with flying.",
    );
    assert_eq!(bladder["modifiers"][0]["blockers"], enchanted());
    // Bower Passage: an attacker query and a blocker collection together.
    let passage = rule(
        "Enchantment",
        "Creatures with flying can't block creatures you control.",
    );
    assert_eq!(
        passage["modifiers"][0]["attackers"]["controller"]["kind"],
        "controllerOf"
    );
}

#[test]
fn temporary_clauses_mark_targets_collections_and_verb_chain_subjects() {
    let stomp = rule("Sorcery", "Creatures without flying can't block this turn.");
    assert_eq!(stomp["effects"][0]["kind"], "grantKeyword");
    assert_eq!(stomp["effects"][0]["keyword"], "cantBlock");
    // An inner duration before `if able` (Shessra's requirement).
    let provoke = rule("Sorcery", "Target creature blocks this turn if able.");
    assert_eq!(provoke["effects"][0]["keyword"], "blockEachCombatIfAble");
    assert_eq!(
        provoke["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    // A coordinated prohibition and a verb chain after a stat change.
    let shackle = rule(
        "Instant",
        "Target creature can't attack or block this turn.",
    );
    assert_eq!(shackle["effects"].as_array().map(Vec::len), Some(2));
    let agents = rule(
        "Creature \u{2014} Human Rogue",
        "{U}{B}: This creature gets -2/-0 until end of turn and can't be blocked this turn.",
    );
    assert!(agents.to_string().contains("cantBeBlocked"), "{agents}");
}

#[test]
fn unrepresentable_neighbours_fail_at_their_own_states() {
    for (type_line, text, state) in [
        // No clause starts without a recurrence or duration: no family claims it.
        ("Enchantment", "Creatures you control attack if able.", ""),
        (
            "Sorcery",
            "Target creature can't be blocked by Walls this turn.",
            "effect.combat-restriction.marker",
        ),
        (
            "Creature \u{2014} Golem",
            "This creature attacks or blocks each combat if able.",
            "combat-clause.requirement",
        ),
    ] {
        assert_eq!(failure_state(type_line, text), state, "{text}");
    }
}
