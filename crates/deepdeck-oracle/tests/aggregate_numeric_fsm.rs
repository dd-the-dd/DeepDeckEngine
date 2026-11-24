//! Aggregate numeric grammar (roadmap #13): counts over any control relation
//! of the shared object query, `the greatest|highest PROPERTY among`, and the
//! distinct basic land types measure, each lowered onto the engine aggregate
//! that evaluates it or refused explicitly where none exists.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

fn rule(type_line: &str, text: &str) -> Value {
    let parsed = parse(type_line, text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.diagnostics
    );
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    ability.rule.clone().expect("canonical rule")
}

/// The first JSON object of the given kind anywhere in the rule.
fn find_kind<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some(kind) {
                return Some(value);
            }
            map.values().find_map(|child| find_kind(child, kind))
        }
        Value::Array(items) => items.iter().find_map(|child| find_kind(child, kind)),
        _ => None,
    }
}

#[test]
fn counts_read_the_query_control_relation_beyond_you_control() {
    let draw = rule(
        "Sorcery",
        "Draw a card for each creature your opponents control.",
    );
    assert_eq!(
        find_kind(&draw, "countPermanentsControlledByOpponents").map(|count| &count["where"]),
        Some(&json!({ "kind": "cardTypeContains", "value": "Creature" })),
        "{draw:#}"
    );
    // The same count in a `where X is` definition (an unused combination).
    let pump = rule(
        "Creature \u{2014} Elf",
        "This creature gets +X/+0, where X is the number of artifacts your opponents control.",
    );
    assert!(
        find_kind(&pump, "countPermanentsControlledByOpponents").is_some(),
        "{pump:#}"
    );
    // The same count over the defending player: an attack trigger records
    // that player as its event player (CR 506.2), so the shared relation
    // counts their permanents with no new code.
    let attack = rule(
        "Creature \u{2014} Human",
        "Whenever this creature attacks, it gets +1/+0 until end of turn for each artifact defending player controls.",
    );
    assert_eq!(
        find_kind(&attack, "countPermanents").map(|count| &count["player"]),
        Some(&json!({ "kind": "triggeringPlayer" })),
        "{attack:#}"
    );
    // Near miss: without an attack event no defending player is recorded,
    // so the card fails explicitly instead of counting the wrong player.
    let parsed = parse(
        "Creature \u{2014} Human",
        "This creature gets +1/+0 for each artifact defending player controls.",
    );
    assert_ne!(parsed.status, "canonical");
}

#[test]
fn greatest_and_highest_read_power_toughness_and_mana_value() {
    let draw = rule(
        "Sorcery",
        "Draw cards equal to the greatest mana value among artifacts you control.",
    );
    assert_eq!(
        find_kind(&draw, "greatestManaValue").map(|value| &value["where"]),
        Some(&json!({ "kind": "cardTypeContains", "value": "Artifact" })),
        "{draw:#}"
    );
    let gain = rule(
        "Sorcery",
        "You gain life equal to the highest power among creatures you control.",
    );
    assert!(find_kind(&gain, "greatestPower").is_some(), "{gain:#}");
}

#[test]
fn basic_land_types_among_lands_you_control_is_the_domain_count() {
    let aura = parse(
        "Enchantment \u{2014} Aura",
        "Enchant creature\nEnchanted creature gets +1/+1 for each basic land type among lands you control.",
    );
    assert_eq!(aura.status, "canonical", "{:#?}", aura.diagnostics);
    assert!(
        aura.abilities
            .iter()
            .filter_map(|ability| ability.rule.as_ref())
            .any(|rule| find_kind(rule, "domainCount").is_some()),
        "{:#?}",
        aura.abilities
    );
    let draw = rule(
        "Sorcery",
        "Draw cards equal to the number of basic land types among lands you control.",
    );
    assert_eq!(
        find_kind(&draw, "domainCount"),
        Some(
            &json!({ "kind": "domainCount", "player": { "kind": "controllerOf", "object": { "kind": "self" } } })
        ),
        "{draw:#}"
    );
    // Near misses: land types counted among non-lands, or among an
    // opponent's lands, have no engine measure.
    for text in [
        "Draw a card for each basic land type among creatures you control.",
        "Draw a card for each basic land type among lands your opponents control.",
    ] {
        assert_ne!(parse("Sorcery", text).status, "canonical", "{text}");
    }
}

#[test]
fn a_comma_list_shortened_mid_list_is_refused_by_its_parent() {
    // A member that does not join the list leaves the comma to the parent;
    // condition, cost and trigger-subject parents refuse it explicitly.
    for (type_line, text) in [
        (
            "Sorcery",
            "If you control an artifact, creatures or planeswalkers, draw a card.",
        ),
        (
            "Artifact",
            "Sacrifice an artifact, creatures or planeswalkers: Draw a card.",
        ),
        (
            "Enchantment",
            "Whenever an artifact, creatures or planeswalkers enter, draw a card.",
        ),
    ] {
        let parsed = parse(type_line, text);
        assert_ne!(parsed.status, "canonical", "{text}");
        assert!(!parsed.diagnostics.is_empty(), "{text}");
    }
}
