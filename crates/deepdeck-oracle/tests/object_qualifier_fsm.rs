//! Object qualifiers (C4): the status-predicate registry, relative clauses,
//! controller/owner relations and chosen values compose in the shared
//! object query and in conditions, never in an action-specific reader.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
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

fn single_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    ability.rule.as_ref().expect("canonical rule")
}

fn rule(type_line: &str, text: &str) -> Value {
    single_rule(&parse("Probe", type_line, text)).clone()
}

#[test]
fn status_adjectives_come_from_one_registry_for_selectors_and_conditions() {
    // Kicked (CR 702.33d): a permanent's status from its kicked spell.
    let kicked = rule(
        "Creature \u{2014} Human",
        "When this creature enters, if it was kicked, draw a card.",
    );
    assert_eq!(
        kicked["condition"],
        json!({
            "kind": "objectMatchesFilter",
            "object": { "kind": "self" },
            "where": { "kind": "hasDesignation", "designation": "kicked" },
        })
    );
    // Equipped / enchanted / modified (CR 301.5, 303.4, 700.9) as a source
    // state, coordinated, and as selector adjectives.
    let either = rule(
        "Creature \u{2014} Human Knight",
        "As long as this creature is enchanted or equipped, this creature has flying.",
    );
    assert_eq!(
        either["condition"]["where"],
        json!({ "kind": "or", "operands": [{ "kind": "hasAttachedAura" }, { "kind": "isEquipped" }] })
    );
    let bounce = rule(
        "Instant",
        "Return target enchanted permanent to its owner's hand.",
    );
    assert_eq!(
        bounce["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "hasAttachedAura" })
    );
    // Sibling: an unused combination, the modified adjective counted.
    let count = rule(
        "Creature \u{2014} Elf",
        "{T}: You gain 1 life for each modified creature you control.",
    );
    assert_eq!(
        count["effects"][0]["amount"]["where"],
        json!({ "kind": "and", "operands": [{ "kind": "isModified" }, { "kind": "cardTypeContains", "value": "Creature" }] })
    );
    // Near misses: a word outside the registry, and a status list mixing
    // both connectors.
    for text in [
        "As long as this creature is shiny, this creature has flying.",
        "As long as this creature is enchanted or equipped and tapped, this creature has flying.",
    ] {
        assert_eq!(
            parse("Near Miss", "Creature \u{2014} Elf", text).status,
            "unsupported",
            "{text}"
        );
    }
}
