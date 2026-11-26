//! Inline resolution choices (`of the creature type of your choice`, `of the
//! color of your choice`): the sequence emits the ordinary `choose`
//! instruction before the clause, and the clause reads the choice through the
//! shared chosen-characteristic predicate. A declared target's characteristic
//! is never chosen on resolution.
use serde_json::Value;

use super::*;
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{3}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

fn effect_rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse(name, type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_eq!(ability.status, "canonical", "{text}: {parsed:#?}");
    ability.rule.clone().expect("canonical rule")
}

fn first_kinds(rule: &Value) -> Vec<String> {
    rule["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .filter_map(|effect| effect["kind"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn the_clause_head_names_its_inline_choice() {
    for (text, domain) in [
        (
            "Creatures of the creature type of your choice get +2/+2 until end of turn.",
            Some(ChoiceDomainNode::CreatureType),
        ),
        (
            "Destroy all creatures of the creature type of your choice.",
            Some(ChoiceDomainNode::CreatureType),
        ),
        (
            "Untap all creatures you control of the creature type of your choice.",
            Some(ChoiceDomainNode::CreatureType),
        ),
        (
            "Return all permanents of the color of your choice to their owners' hands.",
            Some(ChoiceDomainNode::Color),
        ),
        // A declared target's type is chosen as the spell is cast.
        (
            "Return up to three target creature cards of the creature type of your choice from your graveyard to your hand.",
            None,
        ),
        // A preposed condition is not the clause's head noun phrase.
        (
            "If you control a Wizard, creatures of the creature type of your choice get +1/+1 until end of turn.",
            None,
        ),
        // `becomes the creature type of your choice` is the type-change
        // frame's own complement, not a noun qualifier.
        (
            "Target creature becomes the creature type of your choice until end of turn.",
            None,
        ),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("test.inline-choice", &tokens);
        assert_eq!(
            clause_inline_choice(&cursor).map(|choice| choice.domain),
            domain,
            "{text}"
        );
    }
}

#[test]
fn a_group_pump_chooses_the_type_before_its_collection() {
    // Real cards: Witch's Vengeance, Tribal Forcemage (a turned-face-up
    // trigger with a coordinated grant), Walking Desecration (a combat
    // requirement) and Extinction (a one-shot destruction).
    for (name, type_line, text, action) in [
        (
            "Witch's Vengeance",
            "Sorcery",
            "Creatures of the creature type of your choice get -3/-3 until end of turn.",
            "modifyPowerToughness",
        ),
        (
            "Tribal Forcemage",
            "Creature — Elf Wizard",
            "Morph {1}{G}\nWhen this creature is turned face up, creatures of the creature type of your choice get +2/+2 and gain trample until end of turn.",
            "modifyPowerToughness",
        ),
        (
            "Extinction",
            "Sorcery",
            "Destroy all creatures of the creature type of your choice.",
            "destroy",
        ),
    ] {
        let rule = effect_rule(name, type_line, text);
        let kinds = first_kinds(&rule);
        assert_eq!(
            kinds.first().map(String::as_str),
            Some("chooseCreatureType"),
            "{name}: {rule:#}"
        );
        let rendered = rule.to_string();
        assert!(
            rendered.contains("\"chosenCreatureType\""),
            "{name}: {rendered}"
        );
        assert!(
            kinds.iter().any(|kind| kind.starts_with(action)),
            "{name}: {kinds:?}"
        );
    }
}

#[test]
fn the_color_sibling_and_an_unused_combination_compose_without_new_code() {
    // Sibling: the color domain of the same inline choice (Root Greevil's
    // effect), and an unused combination: a combat requirement over the
    // chosen type (Walking Desecration's shape, with `untapped`).
    for (text, choose) in [
        (
            "Destroy all enchantments of the color of your choice.",
            "chooseColor",
        ),
        (
            "Untapped creatures of the creature type of your choice attack this turn if able.",
            "chooseCreatureType",
        ),
    ] {
        let rule = effect_rule("Inline Choice Probe", "Sorcery", text);
        assert_eq!(
            first_kinds(&rule).first().map(String::as_str),
            Some(choose),
            "{text}: {rule:#}"
        );
    }
}

#[test]
fn a_declared_target_of_the_chosen_type_is_refused() {
    // Near miss: no resolution choice is emitted for a target chosen on
    // casting, so the chosen-type reference has no producer.
    let parsed = parse(
        "Near Miss",
        "Instant",
        "Return target creature of the creature type of your choice to its owner's hand.",
    );
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.status != "canonical"),
        "{parsed:#?}"
    );
}
