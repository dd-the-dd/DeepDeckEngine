//! Shared reference grammar: real cards, an unused recombination and near
//! misses for each reference concept. Runtime proofs live in
//! `engine/tests/reference_anaphora_runtime.rs`.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(name: &str, type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
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

fn canonical_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(name, type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
}

fn effects(rule: &Value) -> &[Value] {
    rule["effects"].as_array().map_or(&[], Vec::as_slice)
}

const AURA: &str = "Enchantment \u{2014} Aura";
const EQUIPMENT: &str = "Artifact \u{2014} Equipment";

#[test]
fn attached_object_reference_names_the_permanent_the_source_is_attached_to() {
    let attached = json!({"kind":"attachedPermanent","attachment":{"kind":"self"}});
    // Real cards (Shadow Lance, Torture) and an unused recombination
    // (an Equipment granting a keyword through an activated ability).
    for (type_line, oracle, field) in [
        (
            AURA,
            "Enchant creature\n{1}{B}: Enchanted creature gets +2/+2 until end of turn.",
            "object",
        ),
        (
            AURA,
            "Enchant creature\n{1}{B}: Put a -1/-1 counter on enchanted creature.",
            "permanent",
        ),
        (
            EQUIPMENT,
            "{1}: Equipped creature gains flying until end of turn.\nEquip {2}",
            "object",
        ),
    ] {
        let rules = canonical_rules("Attachment Probe", type_line, oracle);
        assert!(
            rules
                .iter()
                .flat_map(effects)
                .any(|effect| effect[field] == attached),
            "{oracle}: {rules:#?}"
        );
    }
}

#[test]
fn attached_object_reference_requires_the_matching_attachment_source() {
    // Near misses: the role must match the source subtype, and Equipment
    // only equips creatures (CR 301.5).
    for (type_line, oracle) in [
        (
            EQUIPMENT,
            "{1}: Enchanted creature gains flying until end of turn.",
        ),
        (
            "Artifact",
            "{1}: Equipped creature gains flying until end of turn.",
        ),
        (EQUIPMENT, "{1}: Untap equipped land."),
    ] {
        let (status, _, _) = parse("Attachment Miss", type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

#[test]
fn object_relation_subject_reads_the_preceding_singular_object() {
    let controller_of =
        |id: &str| json!({"kind":"controllerOf","object":{"kind":"chosenTarget","id":id}});
    let owner_of = |id: &str| json!({"kind":"ownerOf","object":{"kind":"chosenTarget","id":id}});
    // Real cards (Countermand, Dream Fracture) and unused recombinations:
    // a damage target's controller discarding, a destroyed target's owner.
    for (oracle, player) in [
        (
            "Counter target spell. Its controller mills four cards.",
            controller_of("targetSpell"),
        ),
        (
            "Counter target spell. Its controller draws a card.",
            controller_of("targetSpell"),
        ),
        (
            "Counter target spell. That spell's controller loses 2 life.",
            controller_of("targetSpell"),
        ),
        (
            "Probe deals 2 damage to target creature. That creature's controller discards a card.",
            controller_of("targetPermanent"),
        ),
        (
            "Destroy target creature. Its owner draws a card.",
            owner_of("targetPermanent"),
        ),
    ] {
        let rules = canonical_rules("Probe", "Instant", oracle);
        let actions = effects(&rules[0]);
        assert!(
            actions.iter().any(|effect| effect["player"] == player),
            "{oracle}: {actions:#?}"
        );
    }
}

#[test]
fn object_relation_subject_rejects_unproved_or_renamed_antecedents() {
    // Near misses: no singular object antecedent, and a nominal that the
    // declared selector does not imply.
    for oracle in [
        "Draw a card. Its controller draws a card.",
        "Destroy target artifact. That creature's controller draws a card.",
        "Counter target spell. That creature's controller mills two cards.",
    ] {
        let (status, _, _) = parse("Probe", "Instant", oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

#[test]
fn source_named_by_a_cost_or_a_gendered_pronoun_is_the_antecedent() {
    // Real cards (Triskelion, Smaug the Magnificent) and an unused
    // recombination (a removed-counter cost with a pronoun gaining a keyword).
    for (name, type_line, oracle) in [
        (
            "Probe",
            "Artifact Creature \u{2014} Construct",
            "Remove a +1/+1 counter from this creature: It deals 1 damage to any target.",
        ),
        (
            "Smaug the Magnificent",
            "Legendary Creature \u{2014} Dragon",
            "Whenever Smaug attacks, he deals 2 damage to any target.",
        ),
        (
            "Probe",
            "Creature \u{2014} Beast",
            "Remove a +1/+1 counter from this creature: It gains flying until end of turn.",
        ),
    ] {
        let rules = canonical_rules(name, type_line, oracle);
        let text = serde_json::to_string(&rules).expect("rules serialize");
        assert!(text.contains(r#"{"kind":"self"}"#), "{oracle}: {text}");
    }
    // Near miss: another cost names a second object, so `It` is ambiguous.
    let (status, _, _) = parse(
        "Probe",
        "Creature \u{2014} Beast",
        "Sacrifice another creature, Remove a +1/+1 counter from this creature: It deals 1 damage to any target.",
    );
    assert_ne!(status, "canonical");
}

#[test]
fn defending_player_is_the_attack_event_player() {
    let triggering = json!({"kind":"triggeringPlayer"});
    // Real card (Nemesis of Reason) and an unused recombination (an
    // Equipment attack trigger with a life-loss action).
    for (type_line, oracle) in [
        (
            "Creature \u{2014} Leviathan",
            "Whenever this creature attacks, defending player mills ten cards.",
        ),
        (
            EQUIPMENT,
            "Whenever equipped creature attacks, defending player loses 1 life.\nEquip {2}",
        ),
    ] {
        let rules = canonical_rules("Probe", type_line, oracle);
        assert!(
            rules
                .iter()
                .flat_map(effects)
                .any(|effect| effect["player"] == triggering),
            "{oracle}: {rules:#?}"
        );
    }
    // Near miss: a combat-damage event records the damaged player, not a
    // defending-player binding.
    let (status, _, _) = parse(
        "Probe",
        "Creature \u{2014} Spirit",
        "Whenever this creature deals combat damage to a player, defending player mills two cards.",
    );
    assert_ne!(status, "canonical");
}
