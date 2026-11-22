//! The effect-subject policy: an explicit subject followed by a clause.
//! `you VERB` is the controller's clause, read by the same action readers as
//! the imperative; verbs no reader takes fail at `clause.verb.unsupported`.
use serde_json::Value;

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

fn effects(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .collect()
}

const CREATURE: &str = "Creature \u{2014} Elf";
const SORCERY: &str = "Sorcery";

/// `you VERB` and the imperative `VERB` are one clause: the same IR.
#[test]
fn the_controller_subject_reads_the_imperative_clause() {
    // Real cards: Eivor, Wolf-Kissed (mill), Cruel Ultimatum (a coordinated
    // list continuing the subject). Unused recombinations: scry, surveil and
    // investigate after an explicit `you`.
    for (type_line, explicit, imperative) in [
        (
            CREATURE,
            "Whenever this creature deals combat damage to a player, you mill that many cards.",
            "Whenever this creature deals combat damage to a player, mill that many cards.",
        ),
        (
            SORCERY,
            "Each opponent sacrifices a creature. You return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
            "Each opponent sacrifices a creature. Return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
        ),
        (
            CREATURE,
            "When this creature enters, you scry 2.",
            "When this creature enters, scry 2.",
        ),
        (
            CREATURE,
            "When this creature enters, you surveil 1, then you investigate.",
            "When this creature enters, surveil 1, then investigate.",
        ),
    ] {
        assert_eq!(
            effects(type_line, explicit),
            effects(type_line, imperative),
            "{explicit}"
        );
    }
}

/// An explicit `you` replaces the subject the previous clause would lend a
/// subjectless verb: the controller mills, not the target player.
#[test]
fn the_controller_subject_is_not_the_inherited_player() {
    let effects = effects(
        SORCERY,
        "Target player draws two cards, then you mill two cards.",
    );
    let mill = effects
        .iter()
        .find(|effect| effect["kind"] == "millCards" || effect["kind"] == "mill")
        .unwrap_or_else(|| panic!("a mill effect: {effects:#?}"));
    assert_ne!(mill["player"]["kind"], "targetPlayer", "{mill:#?}");
}

/// Near misses: a verb no action reader takes names its lemma and the
/// subject; modal continuations keep their own frames.
#[test]
fn an_unread_verb_of_the_controller_fails_at_the_clause() {
    let (status, _, states) = parse(
        CREATURE,
        "When this creature enters, you get an emblem with \"Creatures you control get +1/+1.\"",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
    let (_, _, states) = parse(CREATURE, "When this creature enters, you may draw a card.");
    assert!(
        !states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
}
