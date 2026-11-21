//! Keyword actions read as ordinary verbs of the shared effect grammar:
//! doubling counters (CR 701.10e) and proliferate (CR 701.34a).
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rule(name: &str, type_line: &str, text: &str) -> Value {
    let result = parse(name, type_line, text);
    result.abilities[0]
        .rule
        .clone()
        .unwrap_or_else(|| panic!("{name}: {:?}", result.abilities[0].diagnostics))
}

fn failure_state(text: &str) -> String {
    let result = parse("Near Miss", "Sorcery", text);
    assert!(result.abilities[0].rule.is_none(), "{text} must not parse");
    result.abilities[0].diagnostics[0]
        .state
        .clone()
        .unwrap_or_default()
}

#[test]
fn doubling_counters_expands_to_counter_placement_counted_on_the_recipient() {
    let hydra = rule(
        "Mossborn Hydra",
        "Creature — Elemental Hydra",
        "Landfall — Whenever a land you control enters, double the number of +1/+1 counters on this creature.",
    );
    assert_eq!(
        hydra["effects"][0],
        json!({
            "kind": "putCounters",
            "permanent": { "kind": "self" },
            "counter": "+1/+1",
            "count": { "kind": "counterCountOf", "object": { "kind": "self" }, "counter": "+1/+1" },
        })
    );

    let curve = rule(
        "Growth Curve",
        "Sorcery",
        "Put a +1/+1 counter on target creature you control, then double the number of +1/+1 counters on that creature.",
    );
    let effects = curve["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[1]["kind"], "putCounters");
    assert_eq!(effects[1]["permanent"], effects[0]["permanent"]);
    assert_eq!(
        effects[1]["count"],
        json!({ "kind": "counterCountOf", "object": effects[0]["permanent"], "counter": "+1/+1" })
    );
}

#[test]
fn doubling_reuses_the_counter_name_and_target_children_without_new_code() {
    // Unused combination: another counter kind on a fresh target slot.
    let probe = rule(
        "Doubling Probe",
        "Instant",
        "Double the number of charge counters on target artifact.",
    );
    assert_eq!(probe["effects"][0]["counter"], "charge");
    assert_eq!(probe["effects"][0]["count"]["kind"], "counterCountOf");
    assert_eq!(
        probe["effects"][0]["count"]["object"],
        probe["effects"][0]["permanent"]
    );
}

#[test]
fn doubling_near_misses_fail_explicitly() {
    assert_eq!(
        failure_state("Double target creature's power until end of turn."),
        "effect.double.quantity"
    );
    let collection = parse(
        "Near Miss",
        "Sorcery",
        "Double the number of +1/+1 counters on each creature you control.",
    );
    assert!(collection.abilities[0].rule.is_none());
    // `double strike` stays a keyword, not the keyword action.
    let grant = rule(
        "Strike Probe",
        "Instant",
        "Target creature gains double strike until end of turn.",
    );
    assert!(grant.to_string().contains("doubleStrike"));
}

#[test]
fn proliferate_is_an_imperative_player_action() {
    let daggertooth = rule(
        "Urban Daggertooth",
        "Creature — Dinosaur",
        "Enrage — Whenever this creature is dealt damage, proliferate. (Choose any number of permanents and/or players, then give each another counter of each kind already there.)",
    );
    assert_eq!(
        daggertooth["effects"][0],
        json!({ "kind": "proliferate", "player": { "kind": "controllerOf", "object": { "kind": "self" } } })
    );
    // Unused combination: proliferate composed after another ordinary action.
    let probe = rule(
        "Proliferate Probe",
        "Sorcery",
        "Draw a card, then proliferate.",
    );
    let effects = probe["effects"].as_array().expect("effects");
    assert_eq!(effects[0]["kind"], "drawCards");
    assert_eq!(effects[1]["kind"], "proliferate");
}

#[test]
fn proliferate_near_miss_fails_explicitly() {
    let result = parse("Near Miss", "Sorcery", "Proliferate twice.");
    assert!(result.abilities[0].rule.is_none());
}
