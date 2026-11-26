//! `ACTION equal to EXPR` quantities and same-face cost-object bindings,
//! proved on complete cards through the native pipeline (composition and
//! contextual binding included).
use serde_json::{Value, json};

use crate::oracle::model::OracleCardParseRequest;
use crate::oracle::pipeline::parse_oracle_card_native;

fn card(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<(String, Option<Value>)> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    result
        .abilities
        .into_iter()
        .map(|ability| {
            let diagnostic = ability.diagnostics.first();
            let state = diagnostic
                .and_then(|diagnostic| diagnostic.state.clone())
                .unwrap_or_default();
            let message = diagnostic.map(|d| d.message.clone()).unwrap_or_default();
            (
                format!("{}|{}|{}", ability.status, state, message),
                ability.rule,
            )
        })
        .collect()
}

fn canonical_rules(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<Value> {
    card(name, type_line, mana_cost, text)
        .into_iter()
        .map(|(status, rule)| {
            assert!(status.starts_with("canonical"), "{name}: {status}");
            rule.expect("canonical rule")
        })
        .collect()
}

fn find<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
    if value["kind"].as_str() == Some(kind) {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter().find_map(|item| find(item, kind)),
        Value::Object(map) => map.values().find_map(|item| find(item, kind)),
        _ => None,
    }
}

fn sacrificed(id: &str) -> Value {
    json!({ "kind": "decisionResult", "decisionId": id })
}

#[test]
fn fling_binds_the_sacrificed_power_on_its_additional_cost() {
    let rules = canonical_rules(
        "Fling",
        "Instant",
        "{1}{R}",
        "As an additional cost to cast this spell, sacrifice a creature.\nFling deals damage equal to the sacrificed creature's power to any target.",
    );
    let cost = find(&rules[0], "sacrificePermanent").expect("additional sacrifice cost");
    assert_eq!(cost["bindPowerAs"], "sacrificedPermanentPower");
    assert!(cost.get("bindToughnessAs").is_none());
    let damage = find(&rules[1], "dealDamage").expect("damage");
    assert_eq!(damage["amount"], sacrificed("sacrificedPermanentPower"));
}

#[test]
fn activated_sacrifice_binds_toughness_for_a_life_consumer() {
    // Unused combination: activation cost producer, life consumer, toughness.
    let rules = canonical_rules(
        "Binding Probe",
        "Artifact",
        "{2}",
        "Sacrifice a creature: You gain life equal to the sacrificed creature's toughness.",
    );
    let cost = find(&rules[0], "sacrificePermanent").expect("activation sacrifice cost");
    assert_eq!(cost["bindToughnessAs"], "sacrificedPermanentToughness");
    let gain = find(&rules[0], "gainLife").expect("life gain");
    assert_eq!(gain["amount"], sacrificed("sacrificedPermanentToughness"));
}

#[test]
fn a_sacrificed_reference_without_a_same_face_cost_is_rejected() {
    let abilities = card(
        "Binding Probe",
        "Sorcery",
        "{1}{R}",
        "Binding Probe deals damage equal to the sacrificed creature's power to any target.",
    );
    assert!(
        abilities[0]
            .0
            .starts_with("unsupported|binding.sacrificed-cost-object.source|"),
        "{}",
        abilities[0].0
    );
}

#[test]
fn juri_reads_its_own_power_through_the_damage_subject() {
    let rules = canonical_rules(
        "Juri, Master of the Revue",
        "Legendary Creature — Human Shaman",
        "{B}{R}",
        "When Juri dies, it deals damage equal to its power to any target.",
    );
    let damage = find(&rules[0], "dealDamage").expect("damage");
    assert_eq!(
        damage["amount"],
        json!({ "kind": "powerOf", "object": { "kind": "self" } })
    );
}

#[test]
fn postposed_count_and_multiplied_count_share_the_count_child() {
    let rules = canonical_rules(
        "Omnath, Locus of the Roil",
        "Legendary Creature — Elemental",
        "{1}{G}{U}{R}",
        "When Omnath enters, it deals damage to any target equal to the number of Elementals you control.",
    );
    let damage = find(&rules[0], "dealDamage").expect("damage");
    assert_eq!(damage["amount"]["kind"], "countPermanents");
    // Unused combination: twice a count, before the recipient, in a draw.
    let rules = canonical_rules(
        "Count Probe",
        "Sorcery",
        "{2}{U}",
        "Draw cards equal to twice the number of artifacts you control.",
    );
    let draw = find(&rules[0], "drawCards").expect("draw");
    assert_eq!(draw["count"]["kind"], "multiply");
    assert_eq!(draw["count"]["right"]["kind"], "countPermanents");
}

#[test]
fn greatest_power_among_your_creatures_is_an_aggregate_quantity() {
    let rules = canonical_rules(
        "Aggregate Probe",
        "Sorcery",
        "{3}{G}",
        "Draw cards equal to the greatest power among creatures you control.",
    );
    let draw = find(&rules[0], "drawCards").expect("draw");
    assert_eq!(draw["count"]["kind"], "greatestPower");
    // Another controller's permanents: the shared set aggregate.
    let rules = canonical_rules(
        "Aggregate Probe",
        "Sorcery",
        "{3}{G}",
        "Draw cards equal to the greatest power among creatures your opponents control.",
    );
    let draw = find(&rules[0], "drawCards").expect("draw");
    assert_eq!(draw["count"]["kind"], "aggregateObjects");
    assert_eq!(draw["count"]["function"], "greatest");
    assert_eq!(draw["count"]["objects"]["player"]["kind"], "opponentsOf");
}

#[test]
fn its_without_a_damage_subject_is_not_a_source_reference() {
    // `its` names the damage subject only; a draw has none.
    let abilities = card(
        "Pronoun Probe",
        "Sorcery",
        "{2}{U}",
        "Draw cards equal to its power.",
    );
    assert!(
        abilities[0].0.starts_with("unsupported"),
        "{}",
        abilities[0].0
    );
}
