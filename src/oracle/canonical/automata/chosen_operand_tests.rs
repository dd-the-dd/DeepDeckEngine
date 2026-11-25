//! Reference reader and non-target operand grammar: capitalized object
//! vocabulary reaches the shared object query instead of the source-name
//! reader, and an indefinite operand is a resolution choice, never a target.
//! Lowered program shapes live in `effects/chosen_operand_tests.rs`; the
//! runtime proof in `engine/tests/chosen_operand_runtime.rs`.
use serde_json::Value;

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

/// Every JSON object in `value` (depth first), itself included.
fn objects(value: &Value) -> Vec<&Value> {
    let mut found = Vec::new();
    let mut stack = vec![value];
    while let Some(next) = stack.pop() {
        match next {
            Value::Object(fields) => {
                found.push(next);
                stack.extend(fields.values());
            }
            Value::Array(values) => stack.extend(values),
            _ => {}
        }
    }
    found
}

/// Whether `value` holds a subtype test of exactly `subtype` (case
/// sensitive) under a negation (`negated`) or outside any negation.
fn has_subtype_test(value: &Value, subtype: &str, negated: bool) -> bool {
    let is_subtype =
        |object: &Value| object["kind"] == "subtypeContains" && object["value"] == subtype;
    let under_negation = objects(value)
        .into_iter()
        .filter(|object| object["kind"] == "not")
        .any(|not| objects(not).into_iter().any(is_subtype));
    if negated {
        under_negation
    } else {
        objects(value).into_iter().any(is_subtype) && !under_negation
    }
}

#[test]
fn negated_subtype_collection_reaches_the_object_query() {
    // Eyeblight Massacre (real), and the no-code sibling `Non-Human`.
    for (subtype, oracle) in [
        ("Elf", "Non-Elf creatures get -2/-2 until end of turn."),
        ("Human", "Non-Human creatures get -1/-1 until end of turn."),
    ] {
        let rules = canonical_rules("Eyeblight Massacre", "Sorcery", oracle);
        let effect = &rules[0]["effects"][0];
        assert_eq!(effect["kind"], "modifyPowerToughness", "{oracle}");
        assert_eq!(effect["object"]["kind"], "eachPermanent", "{oracle}");
        assert!(
            has_subtype_test(effect, subtype, true),
            "{oracle}: {effect:#}"
        );
    }
    // Polarity: the positive head keeps a positive subtype test.
    let rules = canonical_rules(
        "Positive Elf Probe",
        "Sorcery",
        "Elf creatures get -2/-2 until end of turn.",
    );
    assert!(has_subtype_test(&rules[0]["effects"][0], "Elf", false));
    assert!(!has_subtype_test(&rules[0]["effects"][0], "Elf", true));
}

#[test]
fn supertype_color_and_noncreature_subtype_heads_are_collections() {
    // Battle Frenzy, The Thanos-Copter / Start Your Engines, and an
    // unused recombination (a negated noncreature subtype head).
    for (name, type_line, oracle) in [
        (
            "Battle Frenzy",
            "Instant",
            "Green creatures you control get +1/+1 until end of turn.\nNongreen creatures you control get +1/+0 until end of turn.",
        ),
        (
            "Start Your Engines",
            "Sorcery",
            "Vehicles you control become artifact creatures until end of turn. Creatures you control get +2/+0 until end of turn.",
        ),
        (
            "Snow Probe",
            "Sorcery",
            "Snow creatures you control get +1/+1 until end of turn.",
        ),
        (
            "Non-Vehicle Probe",
            "Sorcery",
            "Non-Vehicle artifacts you control become artifact creatures until end of turn.",
        ),
    ] {
        canonical_rules(name, type_line, oracle);
    }
}

#[test]
fn an_unresolved_card_name_stays_the_source_name_diagnostic() {
    // `Kozilek` is no object vocabulary on a card with another name.
    let (status, _, states) = parse(
        "Kozilek Probe Card",
        "Sorcery",
        "Kozilek gets +1/+1 until end of turn.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "reference.source.unresolved-name"),
        "{states:?}"
    );
}

#[test]
fn an_indefinite_counter_recipient_is_chosen_not_targeted() {
    // Kayla's Command mode, Settle the Score, Sword of Truth and Justice's
    // trigger, and an unused recombination (a loyalty counter on another
    // permanent the controller chooses).
    for (name, type_line, oracle) in [
        (
            "Kayla's Command Mode",
            "Sorcery",
            "Put a +1/+1 counter on a creature you control. It gains double strike until end of turn.",
        ),
        (
            "Settle the Score",
            "Sorcery",
            "Exile target creature. Put two loyalty counters on a planeswalker you control.",
        ),
        (
            "Sword of Truth and Justice",
            "Artifact \u{2014} Equipment",
            "Whenever equipped creature deals combat damage to a player, put a +1/+1 counter on a creature you control, then proliferate.",
        ),
        (
            "Recombination Probe",
            "Sorcery",
            "Put a -1/-1 counter on a creature you control. Draw a card.",
        ),
    ] {
        let rules = canonical_rules(name, type_line, oracle);
        let rule = &rules[0];
        let choose = objects(rule)
            .into_iter()
            .find(|object| object["kind"] == "choosePermanents")
            .unwrap_or_else(|| panic!("{oracle}: no resolution choice in {rule:#}"));
        let id = choose["id"].as_str().expect("choice id");
        // No target is declared for the recipient.
        assert!(
            !objects(&rule["declaration"])
                .into_iter()
                .any(|object| object["id"] == id),
            "{oracle}"
        );
        let put = objects(rule)
            .into_iter()
            .find(|object| object["kind"] == "putCounters")
            .expect("a counter placement");
        assert_eq!(put["permanent"]["kind"], "singleBoundObject", "{oracle}");
    }
}

#[test]
fn a_chosen_attach_destination_reads_the_moved_equipment() {
    // Stonehewer Giant / Quest for the Holy Relic.
    let rules = canonical_rules(
        "Stonehewer Giant",
        "Creature \u{2014} Giant Warrior",
        "Vigilance\n{1}{W}, {T}: Search your library for an Equipment card, put it onto the battlefield, attach it to a creature you control, then shuffle.",
    );
    let attach = rules
        .iter()
        .flat_map(objects)
        .find(|object| object["kind"] == "attachPermanent")
        .expect("an attachment");
    assert_eq!(attach["to"]["kind"], "singleBoundObject");
    assert_ne!(attach["to"], attach["attachment"]);
}

#[test]
fn a_target_head_stays_a_declaration_and_an_unread_tail_fails() {
    // Near misses: `a target creature` is no resolution choice, and a choice
    // of an articled list is one choice per head, never one operand.
    let declared = canonical_rules(
        "Declared Probe",
        "Sorcery",
        "Put a +1/+1 counter on target creature.",
    );
    assert!(
        !objects(&declared[0])
            .into_iter()
            .any(|object| object["kind"] == "choosePermanents")
    );
    let (status, _, _) = parse(
        "Articled List Probe",
        "Sorcery",
        "Put a +1/+1 counter on an artifact, a creature, or a land you control.",
    );
    assert_ne!(status, "canonical");
}
