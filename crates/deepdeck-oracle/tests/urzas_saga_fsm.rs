use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI — This Saga gains \"{T}: Add {C}.\"\nII — This Saga gains \"{2}, {T}: Create a 0/0 colorless Construct artifact creature token with 'This token gets +1/+1 for each artifact you control.'\"\nIII — Search your library for an artifact card with mana cost {0} or {1}, put it onto the battlefield, then shuffle.";

fn parse(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Saga Parser Fixture".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("saga".to_string()),
        faces: Vec::new(),
    })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn urzas_saga_is_entirely_native_and_primitive() {
    let parsed = parse("Enchantment Land — Urza's Saga", EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 4);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));

    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "rulesMarker",
            "source": { "kind": "self" },
            "marker": "sagaProgression",
            "finalChapter": { "kind": "integer", "value": 3 },
        }))
    );

    let first = parsed.abilities[1].rule.as_ref().expect("chapter I");
    assert_eq!(
        first["event"]["chapters"],
        json!([{ "kind": "integer", "value": 1 }])
    );
    assert_eq!(first["effects"][0]["kind"], "grantAbility");
    assert_eq!(first["effects"][0]["duration"]["kind"], "permanent");
    assert_eq!(first["effects"][0]["ability"]["kind"], "manaAbility");
    assert_eq!(first["effects"][0]["ability"]["effects"][0]["mana"], "{C}");

    // Chapter II is the shared keyword-grant clause: the quoted activated
    // ability is compiled recursively, and its token's single-quoted static
    // ability is compiled by the same quoted-ability slot.
    let second = parsed.abilities[2].rule.as_ref().expect("chapter II");
    assert_eq!(second["effects"][0]["kind"], "grantAbility");
    assert_eq!(second["effects"][0]["duration"]["kind"], "permanent");
    let created = &second["effects"][0]["ability"]["effects"][0];
    assert_eq!(created["kind"], "createTokens", "{second:#}");
    assert_eq!(created["controller"], controller());
    assert_eq!(created["token"]["colors"], json!([]));
    assert_eq!(created["token"]["types"], json!(["Artifact", "Creature"]));
    assert_eq!(created["token"]["subtypes"], json!(["Construct"]));
    let characteristic = &created["token"]["abilities"][0];
    assert_eq!(
        characteristic["kind"], "staticAbility",
        "{characteristic:#}"
    );
    assert!(
        characteristic.to_string().contains("\"Artifact\""),
        "{characteristic:#}"
    );

    let third = parsed.abilities[3].rule.as_ref().expect("chapter III");
    assert_eq!(
        third["effects"]
            .as_array()
            .expect("primitive effects")
            .iter()
            .map(|effect| effect["kind"].as_str().expect("effect kind"))
            .collect::<Vec<_>>(),
        vec![
            "chooseCards",
            "moveCards",
            "shuffleZone",
            "recordLibrarySearch"
        ]
    );
    assert_eq!(
        third["effects"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Artifact" },
                {
                    "kind": "or",
                    "operands": [
                        { "kind": "manaCostEquals", "value": "{0}" },
                        { "kind": "manaCostEquals", "value": "{1}" },
                    ],
                },
            ],
        })
    );
    assert!(!third.to_string().contains("searchLibrary"));
}

#[test]
fn noncontiguous_chapters_green_mana_and_other_type_costs_reuse_the_same_automata() {
    let grant = parse(
        "Enchantment — Saga",
        "I, III — This Saga gains \"{T}: Add {G}.\"",
    );
    assert_eq!(grant.status, "canonical");
    assert_eq!(grant.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        grant.abilities[0].rule.as_ref().unwrap()["event"]["chapters"],
        json!([
            { "kind": "integer", "value": 1 },
            { "kind": "integer", "value": 3 },
        ])
    );
    assert_eq!(
        grant.abilities[0].rule.as_ref().unwrap()["effects"][0]["ability"]["effects"][0]["mana"],
        "{G}"
    );

    let search = parse(
        "Enchantment — Saga",
        "V — Search your library for an enchantment card with mana cost {2} or {3}, put it onto the battlefield, then shuffle.",
    );
    assert_eq!(search.status, "canonical");
    assert_eq!(search.abilities[0].recognizer, RecognizerProvenance::Native);
    let predicate =
        &search.abilities[0].rule.as_ref().unwrap()["effects"][0]["candidates"]["where"];
    assert_eq!(predicate["operands"][0]["value"], "Enchantment");
    assert_eq!(predicate["operands"][1]["operands"][0]["value"], "{2}");
    assert_eq!(predicate["operands"][1]["operands"][1]["value"], "{3}");
}

#[test]
fn malformed_saga_members_fail_in_the_responsible_native_state() {
    for (oracle_text, automaton, state) in [
        (
            "II, — This Saga gains \"{T}: Add {G}.\"",
            "saga-chapter",
            "saga.chapter.roman",
        ),
        (
            "III — Search your library for an artifact card with mana cost {0} or, put it onto the battlefield, then shuffle.",
            "saga-chapter",
            "cost.mana.start",
        ),
    ] {
        let parsed = parse("Enchantment — Saga", oracle_text);
        assert_eq!(parsed.status, "unsupported", "{oracle_text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some(automaton),
            "{oracle_text}"
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle_text}");
    }
    // Unclosed quotes (outer or nested) fail inside the shared quoted-ability
    // slot, never in a Saga-private grant grammar.
    for oracle_text in [
        "I — This Saga gains \"{T}: Add {G}.",
        "II — This Saga gains \"{2}, {T}: Create a 0/0 colorless Construct artifact creature token with 'This token gets +1/+1 for each artifact you control.\"",
    ] {
        let parsed = parse("Enchantment — Saga", oracle_text);
        assert_eq!(parsed.status, "unsupported", "{oracle_text}");
        assert!(
            parsed.diagnostics.iter().all(|diagnostic| !diagnostic
                .state
                .as_deref()
                .is_some_and(|state| state.starts_with("saga.chapter.grant"))),
            "{oracle_text}: {:#?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn roman_header_on_a_non_saga_type_line_is_not_claimed() {
    let parsed = parse("Enchantment", "I, III — This Saga gains \"{T}: Add {G}.\"");
    // Structural frames now claim every ability explicitly: the near-miss is
    // a committed ability-frame failure, never stolen by the adjacent family.
    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(ability.diagnostics[0].code, "oracle_machine_failure");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("ability-frame")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("ability-frame.static.subject")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("I"));
}
