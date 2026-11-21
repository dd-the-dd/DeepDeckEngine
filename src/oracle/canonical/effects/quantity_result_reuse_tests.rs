//! Quantities and result reuse (C4): zone-scoped counts in the shared
//! counted collection, the static `+X/+Y, where X is` delta, and counts of an
//! earlier action's affected objects (`for each creature destroyed this way`).
use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, ParsedOracleAbility, parse_oracle_card};

fn card(name: &str, type_line: &str, oracle: &str) -> Vec<ParsedOracleAbility> {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
    .abilities
}

fn rule(name: &str, type_line: &str, oracle: &str) -> Value {
    let abilities = card(name, type_line, oracle);
    let ability = abilities.last().expect("one ability");
    assert_eq!(ability.status, "canonical", "{name}: {abilities:#?}");
    ability.rule.clone().expect("canonical rule")
}

fn failure_state(name: &str, type_line: &str, oracle: &str) -> String {
    let abilities = card(name, type_line, oracle);
    let ability = abilities.last().expect("one ability");
    assert_ne!(
        ability.status, "canonical",
        "{name} must not parse: {abilities:#?}"
    );
    ability.diagnostics[0].state.clone().unwrap_or_default()
}

fn find<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some(kind) {
                return Some(value);
            }
            map.values().find_map(|child| find(child, kind))
        }
        Value::Array(items) => items.iter().find_map(|child| find(child, kind)),
        _ => None,
    }
}

#[test]
fn zone_scoped_counts_lower_to_the_shared_card_count() {
    // Real cards: an action amount, a mana production and a static delta.
    let castle = rule(
        "Castle Locthwain",
        "Legendary Land",
        "{1}{B}{B}, {T}: Draw a card, then you lose life equal to the number of cards in your hand.",
    );
    let count = find(&castle, "countCards").expect("hand count");
    assert_eq!(count["zone"]["kind"], "hand");
    assert!(count.get("allPlayers").is_none());

    let songs = rule(
        "Songs of the Damned",
        "Instant",
        "Add {B} for each creature card in your graveyard.",
    );
    let count = find(&songs, "countCards").expect("graveyard count");
    assert_eq!(count["zone"]["kind"], "graveyard");
    assert_eq!(count["where"]["kind"], "cardTypeContains");

    let knight = rule(
        "Knight of the Reliquary",
        "Creature — Human Knight",
        "This creature gets +1/+1 for each land card in your graveyard.",
    );
    assert_eq!(knight["modifiers"][0]["power"]["kind"], "countCards");

    // CR 110.1: `on the battlefield` counts every controller's permanents.
    let priest = rule(
        "Priest of Titania",
        "Creature — Elf Druid",
        "{T}: Add {G} for each Elf on the battlefield.",
    );
    let count = find(&priest, "countPermanents").expect("battlefield count");
    assert_eq!(count["allPlayers"], true);
}

#[test]
fn unused_zone_count_combinations_need_no_new_production() {
    // Draw × library count, damage × all-graveyards count, subtype card filter.
    let draw = rule(
        "Sibling Draw",
        "Sorcery",
        "Draw a card for each artifact card in your library.",
    );
    assert_eq!(
        find(&draw, "countCards").unwrap()["zone"]["kind"],
        "library"
    );
    let damage = rule(
        "Sibling Damage",
        "Sorcery",
        "Sibling Damage deals 1 damage to any target for each Zombie card in all graveyards.",
    );
    let count = find(&damage, "countCards").expect("all-graveyard count");
    assert_eq!(count["allPlayers"], true);
    assert_eq!(count["where"]["kind"], "subtypeContains");
}

#[test]
fn zone_counts_without_an_evaluable_holder_fail_explicitly() {
    // A targeted or per-player holder has no typed producer in this child.
    assert_eq!(
        failure_state(
            "Near Holder",
            "Sorcery",
            "Each player draws a card for each creature card in their graveyard.",
        ),
        "query.zone.holder"
    );
}

#[test]
fn static_where_x_delta_substitutes_the_continuous_definition() {
    let fear = rule(
        "Fear of Death",
        "Enchantment — Aura",
        "Enchant creature\nEnchanted creature gets -X/-0, where X is the number of cards in your graveyard.",
    );
    let modifier = &fear["modifiers"][0];
    assert_eq!(modifier["kind"], "modifyPowerToughness");
    assert_eq!(modifier["power"]["kind"], "subtract");
    assert_eq!(modifier["power"]["right"]["kind"], "countCards");
    assert_eq!(modifier["toughness"]["kind"], "subtract");

    let shadow = rule(
        "Death's Shadow",
        "Creature — Avatar",
        "This creature gets -X/-X, where X is your life total.",
    );
    assert_eq!(
        shadow["modifiers"][0]["toughness"]["right"]["kind"],
        "lifeTotal"
    );

    // Unused combination: a controlled collection with a permanent count.
    let anthem = rule(
        "Sibling Anthem",
        "Enchantment",
        "Creatures you control get +X/+0, where X is the number of Elves on the battlefield.",
    );
    assert_eq!(anthem["modifiers"][0]["power"]["kind"], "countPermanents");
    assert_eq!(anthem["modifiers"][0]["toughness"]["kind"], "integer");
}

#[test]
fn static_symbolic_delta_without_its_definition_is_refused() {
    assert_eq!(
        failure_state(
            "Near Static",
            "Creature — Avatar",
            "This creature gets +X/+X until end of turn, where X is the number of cards in your hand.",
        ),
        "static.continuous.where-x"
    );
}

#[test]
fn destroyed_this_way_counts_the_bound_result_of_the_destroy_action() {
    let fumigate = rule(
        "Fumigate",
        "Sorcery",
        "Destroy all creatures. You gain 1 life for each creature destroyed this way.",
    );
    let effects = fumigate["effects"].as_array().expect("effects");
    assert_eq!(effects[0]["kind"], "destroyPermanent");
    assert_eq!(effects[0]["bind"], "actionResult:0");
    assert_eq!(effects[1]["kind"], "gainLife");
    assert_eq!(effects[1]["amount"]["kind"], "countObjects");
    assert_eq!(effects[1]["amount"]["objects"]["binding"], "actionResult:0");

    // A creature with flying is a creature: the noun is guaranteed.
    let hydra = rule(
        "Whiptongue Hydra",
        "Creature — Lizard Hydra",
        "Reach\nWhen this creature enters, destroy all creatures with flying. Put a +1/+1 counter on this creature for each creature destroyed this way.",
    );
    assert_eq!(
        find(&hydra, "destroyPermanent").unwrap()["bind"],
        "actionResult:0"
    );
    assert_eq!(
        find(&hydra, "putCounters").unwrap()["count"]["objects"]["kind"],
        "boundObjects"
    );
}

#[test]
fn milled_this_way_filters_the_milled_cards() {
    let vulture = rule(
        "Gorging Vulture",
        "Creature — Bird",
        "Flying\nWhen this creature enters, mill four cards. You gain 1 life for each creature card milled this way.",
    );
    assert_eq!(find(&vulture, "mill").unwrap()["bind"], "actionResult:0");
    let filtered = find(&vulture, "filterObjects").expect("card filter over the milled cards");
    assert_eq!(filtered["objects"]["binding"], "actionResult:0");
    assert_eq!(filtered["where"]["kind"], "cardTypeContains");
}

#[test]
fn unused_result_reuse_combination_targets_another_action() {
    // Destroy artifacts, then counters on a target: no new production code.
    let sibling = rule(
        "Sibling Result",
        "Sorcery",
        "Destroy all artifacts. Put a +1/+1 counter on target creature for each artifact destroyed this way.",
    );
    assert_eq!(
        find(&sibling, "destroyPermanent").unwrap()["bind"],
        "actionResult:0"
    );
    assert_eq!(
        find(&sibling, "putCounters").unwrap()["count"]["objects"]["binding"],
        "actionResult:0"
    );
}

#[test]
fn result_reuse_without_a_guaranteeing_antecedent_is_refused() {
    // `Zombie` is not guaranteed by `creatures`; tokens no longer exist to filter.
    assert_eq!(
        failure_state(
            "Near Noun",
            "Sorcery",
            "Destroy all creatures. You gain 1 life for each Zombie destroyed this way.",
        ),
        "sequence.for-each.action-result.antecedent"
    );
    // No earlier destroy action produces the restated objects.
    assert_eq!(
        failure_state(
            "Near Antecedent",
            "Sorcery",
            "You gain 1 life for each creature destroyed this way.",
        ),
        "sequence.for-each.action-result.antecedent"
    );
}
