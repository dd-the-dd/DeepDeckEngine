//! Saga chapters (roadmap #2, CR 715.2b): the chapter header owns the Roman
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

#[test]
fn a_modal_chapter_reads_its_modes_like_a_modal_trigger() {
    // The shared triggered-body helper: `Choose one —` modes are chosen as
    // the chapter ability is put on the stack (CR 700.2b).
    let rules = chapter_rules(
        "I \u{2014} Choose one \u{2014}\n\u{2022} You gain 3 life.\n\u{2022} Draw a card.\nII, III \u{2014} Draw a card.",
    );
    assert_eq!(rules.len(), 2, "{rules:#?}");
    assert_eq!(rules[0]["event"]["kind"], "sagaChapterReached");
    assert!(
        rules[0].to_string().contains("\"gainLife\""),
        "{:#}",
        rules[0]
    );
}

#[test]
fn die_roll_result_rows_are_never_unconditional_abilities() {
    // Diviner's Portent: `15+ |` rows are die-roll results, not station
    // bands; whatever owns them, they are not accepted as unconditional.
    let request = OracleCardParseRequest {
        card_name: "Diviner's Portent".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{3}{U}{U}".to_string()),
        oracle_text: Some(
            "Roll a d20 and add the number of cards in your hand.\n1\u{2014}14 | Draw X cards.\n15+ | Scry X, then draw X cards."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser");
    assert_ne!(parsed.status, "canonical");
    for row in [
        "1\u{2014}14 | Draw X cards.",
        "15+ | Scry X, then draw X cards.",
    ] {
        let ability = parsed
            .abilities
            .iter()
            .find(|ability| ability.source.text == row)
            .unwrap_or_else(|| panic!("{row}: {:#?}", parsed.abilities));
        assert_ne!(ability.status, "canonical", "{row}");
        assert!(ability.rule.is_none(), "{row}");
        // The row's result number is not an effect subject.
        assert_eq!(
            ability.diagnostics[0].state.as_deref(),
            Some("effect.start"),
            "{row}: {:#?}",
            ability.diagnostics
        );
        // Not mislabelled as a station band body either.
        assert!(
            ability
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "ungated_band_body"),
            "{row}"
        );
    }
}

fn parse_card(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("leveler".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

#[test]
fn band_bodies_are_never_accepted_without_their_gate() {
    // Halimar Wavewatch (CR 711.2): the headers are introducers, not
    // abilities; every body line, P/T boxes included, carries its gate.
    let parsed = parse_card(
        "Halimar Wavewatch",
        "Creature \u{2014} Merfolk Wizard",
        "Level up {2}\nLEVEL 1-4\n0/6\nLEVEL 5+\n6/6\nIslandwalk",
    );
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| !ability.source.text.starts_with("LEVEL")),
        "{:#?}",
        parsed.abilities
    );
    for (text, minimum, maximum) in [
        ("0/6", 1, Some(4)),
        ("6/6", 5, None),
        ("Islandwalk", 5, None),
    ] {
        let ability = parsed
            .abilities
            .iter()
            .find(|ability| ability.source.text == text)
            .unwrap_or_else(|| panic!("{text}: {:#?}", parsed.abilities));
        let Some(rule) = &ability.rule else {
            // An unsupported body keeps its own diagnostic; it is never
            // accepted ungated.
            assert_ne!(ability.status, "canonical", "{text}");
            continue;
        };
        assert_eq!(rule["bandGate"]["kind"], "counterRange", "{text}");
        assert_eq!(rule["bandGate"]["counter"], "level", "{text}");
        assert_eq!(rule["bandGate"]["minimum"], minimum, "{text}");
        match maximum {
            Some(maximum) => assert_eq!(rule["bandGate"]["maximum"], maximum, "{text}"),
            None => assert!(rule["bandGate"].get("maximum").is_none(), "{text}"),
        }
    }
    let statistics = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text == "6/6")
        .and_then(|ability| ability.rule.as_ref())
        .expect("a level P/T box is a gated static");
    assert_eq!(
        statistics["modifiers"][0]["kind"], "setBasePowerToughness",
        "{statistics:#}"
    );
}

#[test]
fn station_and_solved_bodies_parse_without_their_introducer() {
    // An unused combination: a station band body is an ordinary triggered
    // ability once its `N+ |` introducer is removed.
    let parsed = parse_card(
        "Probe Spacecraft",
        "Artifact \u{2014} Spacecraft",
        "Station\n3+ | Whenever you cast an instant or sorcery spell, you gain 1 life.",
    );
    let body = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.starts_with("Whenever"))
        .unwrap_or_else(|| panic!("{:#?}", parsed.abilities));
    if let Some(rule) = &body.rule {
        assert_eq!(
            rule["bandGate"],
            json!({ "kind": "counterRange", "counter": "charge", "minimum": 3 })
        );
    }
    let parsed = parse_card(
        "Probe Case",
        "Enchantment \u{2014} Case",
        "Solved \u{2014} Creatures you control get +1/+0.",
    );
    let body = &parsed.abilities[0];
    assert_eq!(body.source.text, "Creatures you control get +1/+0.");
    if let Some(rule) = &body.rule {
        assert_eq!(rule["bandGate"]["kind"], "designation");
    }
}
