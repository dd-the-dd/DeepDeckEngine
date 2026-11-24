use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const FRONT_ORACLE: &str = "Flying\nWhenever Tamiyo attacks, investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")\nWhen you draw your third card in a turn, exile Tamiyo, then return her to the battlefield transformed under her owner's control.";

fn parse(name: &str, oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Legendary Creature — Moonfolk Wizard".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rule(parsed: &OracleCardParseResult, needle: &str) -> Value {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(needle))
        .and_then(|ability| ability.rule.clone())
        .expect("native front-face rule")
}

#[test]
fn tamiyo_front_face_triggers_are_native_primitives() {
    let truth: Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/tamiyo-inquisitive-student.json"
    ))
    .expect("Tamiyo front-face ground truth is valid JSON");
    let parsed = parse("Tamiyo, Inquisitive Student", FRONT_ORACLE);

    for (needle, truth_index) in [("attacks", 0), ("third card", 1)] {
        let ability = parsed
            .abilities
            .iter()
            .find(|ability| ability.source.text.contains(needle))
            .expect("front-face ability exists");
        assert_eq!(ability.status, "canonical");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(
            ability.rule,
            Some(truth["abilities"][truth_index]["expectedRule"].clone())
        );
    }
    let serialized = serde_json::to_string(&parsed).expect("parse result serializes");
    assert!(!serialized.contains("exileThenReturnTransformed"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn source_anaphor_attack_trigger_reuses_the_investigate_child() {
    let attack = parse(
        "Curious Scout",
        "Whenever this creature attacks, investigate.",
    );
    assert_eq!(attack.status, "canonical");
    assert_eq!(attack.abilities[0].recognizer, RecognizerProvenance::Native);
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Tamiyo, Inquisitive Student; see docs/coverage-targets.md"
)]
#[test]
fn source_anaphor_and_arbitrary_ordinal_reuse_the_same_children() {
    let transform = parse(
        "Changing Student",
        "When you draw your fourth card in a turn, exile this creature, then return it to the battlefield transformed under its owner's control.",
    );
    assert_eq!(transform.status, "canonical");
    assert_eq!(
        transform.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        rule(&transform, "fourth card")["event"]["drawOrdinal"],
        json!({"kind":"integer","value":4})
    );
}

#[test]
fn malformed_effect_and_counted_noun_are_committed_failures() {
    let investigate = parse(
        "Curious Scout",
        // `investigate twice` is the shared repetition adjunct; an
        // unbounded repetition has no amount it can read.
        "Whenever this creature attacks, investigate any number of times.",
    );
    assert_eq!(investigate.status, "unsupported");
    assert_eq!(
        investigate.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert!(investigate.abilities[0].rule.is_none());

    let ordinal = parse(
        "Changing Student",
        "When you draw your third spell in a turn, exile this creature, then return it to the battlefield transformed under its owner's control.",
    );
    assert_eq!(ordinal.status, "unsupported");
    assert_eq!(
        ordinal.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert!(ordinal.abilities[0].rule.is_none());
    assert_eq!(
        ordinal.abilities[0].diagnostics[0].found.as_deref(),
        Some("spell")
    );
}
