//! The numeric-expression family (C7): one arithmetic grammar reads counts,
//! player quantities, devotion and halves with their stated rounding.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{B}".to_string()),
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
    parsed.abilities[0].rule.clone().expect("canonical rule")
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn devotion_and_halves_are_operands_of_the_shared_arithmetic_grammar() {
    // Devotion (CR 700.5) as a postposed definition of X.
    let merchant = rule(
        "Creature \u{2014} Zombie",
        "When this creature enters, each opponent loses X life, where X is your devotion to black. You gain life equal to the life lost this way.",
    );
    assert_eq!(
        merchant["effects"][0]["value"],
        json!({ "kind": "devotion", "player": controller(), "color": "Black" })
    );
    // Sibling: devotion as an `equal to` amount of another action.
    let draw = rule("Sorcery", "Draw cards equal to your devotion to blue.");
    assert_eq!(draw["effects"][0]["count"]["kind"], "devotion");
    // `half EXPR, rounded down` (CR 107.1a) over a player quantity.
    let half = rule(
        "Sorcery",
        "You gain life equal to half your life total, rounded down.",
    );
    assert_eq!(
        half["effects"][0]["amount"],
        json!({
            "kind": "divide",
            "left": { "kind": "lifeTotal", "player": controller() },
            "right": { "kind": "integer", "value": 2 },
            "round": "down",
        })
    );
    // Near misses: a half without its stated rounding, and a devotion to
    // two colors (hybrid symbols count once, CR 700.5a).
    for text in [
        "You gain life equal to half your life total.",
        "Draw cards equal to your devotion to black and red.",
    ] {
        assert_eq!(parse("Sorcery", text).status, "unsupported", "{text}");
    }
}
