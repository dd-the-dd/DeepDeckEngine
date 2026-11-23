//! Saga chapters (roadmap #2, CR 714.2b): the chapter header owns the Roman
//! numeral list and the dash; the body is the shared effect program every
//! triggered ability reads, lowered under the `sagaChapterReached` event.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

const REMINDER: &str =
    "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Probe Saga".to_string(),
        type_line: "Enchantment \u{2014} Saga".to_string(),
        mana_cost: Some("{2}{W}".to_string()),
        oracle_text: Some(format!("{REMINDER}\n{oracle_text}")),
        layout: Some("saga".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

fn chapter_rules(oracle_text: &str) -> Vec<Value> {
    let parsed = parse(oracle_text);
    assert_eq!(
        parsed.status, "canonical",
        "{oracle_text}: {:#?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.clone())
        .filter(|rule| rule["kind"] == "triggeredAbility")
        .collect()
}

fn chapters(rule: &Value) -> Value {
    rule["event"]["chapters"].clone()
}

#[test]
fn chapter_bodies_are_ordinary_effect_programs() {
    let rules = chapter_rules(
        "I \u{2014} You gain 3 life.\nII \u{2014} Draw a card, then discard a card.\nIII \u{2014} Create a 1/1 white Soldier creature token.",
    );
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0]["event"]["kind"], "sagaChapterReached");
    assert_eq!(
        chapters(&rules[0]),
        json!([{ "kind": "integer", "value": 1 }])
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "gainLife");
    assert_eq!(rules[1]["effects"][0]["kind"], "drawCards");
    assert_eq!(rules[2]["effects"][0]["kind"], "createTokens");
}

#[test]
fn an_unused_combination_declares_targets_on_a_grouped_chapter() {
    // A grouped chapter header with a targeted program body: the target is
    // declared by the chapter trigger like any triggered ability's target.
    let rules = chapter_rules(
        "I, II \u{2014} Target creature an opponent controls gets -2/-2 until end of turn.\nIII \u{2014} Each opponent loses 2 life and you gain 2 life.",
    );
    assert_eq!(
        chapters(&rules[0]),
        json!([{ "kind": "integer", "value": 1 }, { "kind": "integer", "value": 2 }])
    );
    assert!(
        rules[0]["declaration"]["decisions"]
            .as_array()
            .is_some_and(|decisions| !decisions.is_empty()),
        "{:#}",
        rules[0]
    );
    assert_eq!(rules[1]["effects"].as_array().map(Vec::len), Some(2));
}

#[test]
fn malformed_headers_and_unknown_bodies_keep_explicit_failures() {
    // Near miss: a decreasing chapter list fails in the header, before any
    // body is read.
    let parsed = parse("II, I \u{2014} You gain 3 life.");
    assert_ne!(parsed.status, "canonical");
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.state.as_deref() == Some("saga.chapter.roman")
                    || diagnostic.expected.as_deref()
                        == Some("strictly increasing, nonduplicated Saga chapter numerals")
            ),
        "{:#?}",
        parsed.diagnostics
    );
    // An unknown body fails inside the shared program, not as a closed
    // `native Saga chapter body` list.
    let parsed = parse("I \u{2014} Frobnicate the moon.");
    assert_ne!(parsed.status, "canonical");
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("a native Saga chapter body")),
        "{:#?}",
        parsed.diagnostics
    );
}
