use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const DISMEMBER_ORACLE: &str =
    "({B/P} can be paid with either {B} or 2 life.)\nTarget creature gets -5/-5 until end of turn.";

fn parse(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B/P}{B/P}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn effect_rule(parsed: &OracleCardParseResult) -> &serde_json::Value {
    parsed
        .abilities
        .iter()
        .find_map(|ability| {
            ability
                .rule
                .as_ref()
                .filter(|rule| rule["kind"] == "spellAbility")
        })
        .expect("spell effect rule")
}

#[test]
fn dismember_is_fully_native_primitive_ir() {
    let parsed = parse("Dismember", DISMEMBER_ORACLE);

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(parsed.abilities.iter().all(|ability| {
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    }));

    let rule = effect_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "permanents",
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        })
    );
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][0]["power"]["value"], -5);
    assert_eq!(rule["effects"][0]["toughness"]["value"], -5);
    assert_eq!(
        rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn literal_deltas_selectors_and_keyword_atoms_generalize_independently() {
    for (oracle, power, toughness, keywords) in [
        (
            "Target creature gets -2/-2 until end of turn.",
            -2,
            -2,
            Vec::<&str>::new(),
        ),
        (
            "Target creature gets +3/+3 until end of turn.",
            3,
            3,
            Vec::<&str>::new(),
        ),
        (
            "Target artifact creature gets +2/+2 and gains flying and vigilance until end of turn.",
            2,
            2,
            vec!["flying", "vigilance"],
        ),
    ] {
        let parsed = parse("Test Modifier", oracle);
        assert_eq!(parsed.status, "canonical", "{oracle}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle}"
        );
        let rule = effect_rule(&parsed);
        assert_eq!(rule["effects"][0]["power"]["value"], power, "{oracle}");
        assert_eq!(
            rule["effects"][0]["toughness"]["value"], toughness,
            "{oracle}"
        );
        assert_eq!(
            rule["effects"]
                .as_array()
                .expect("effects")
                .iter()
                .skip(1)
                .map(|effect| effect["keyword"].as_str().expect("keyword"))
                .collect::<Vec<_>>(),
            keywords,
            "{oracle}"
        );
    }
}

#[test]
fn promoted_family_reports_near_misses_without_legacy_fallback() {
    for (oracle, expected_state, expected_found) in [
        (
            "Target creature gets -2/-2.",
            "lower.modify-stats.indefinite-duration",
            None,
        ),
        (
            "Target creature gets -2/-2 until end of combat.",
            "duration.until-end-of-turn.unit",
            Some("combat"),
        ),
        (
            "Target spell gets -2/-2 until end of turn.",
            // The clause's deepest component: target subject, lemma `get`.
            "effect.clause/subject.target.verb.get",
            Some("gets"),
        ),
    ] {
        let parsed = parse("Test Modifier", oracle);
        assert_eq!(parsed.status, "unsupported", "{oracle}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle}"
        );
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("contextual-effect-program"),
            "{oracle}"
        );
        assert_eq!(
            diagnostic.state.as_deref(),
            Some(expected_state),
            "{oracle}"
        );
        assert_eq!(diagnostic.found.as_deref(), expected_found, "{oracle}");
    }
}

#[test]
fn temporary_modifier_composes_with_a_following_independent_action() {
    let parsed = parse(
        "Test Modifier",
        "Target creature gets -2/-2 until end of turn. Draw a card.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let effects = &effect_rule(&parsed)["effects"];
    assert_eq!(effects.as_array().map(Vec::len), Some(2));
    assert_eq!(effects[0]["kind"], "modifyPowerToughness");
    assert_eq!(effects[0]["power"]["value"], -2);
    assert_eq!(effects[0]["duration"]["kind"], "untilEndOfCurrentTurn");
    assert_eq!(effects[1]["kind"], "drawCards");
    assert_eq!(effects[1]["count"]["value"], 1);
}

#[test]
fn another_target_exclusion_is_preserved_in_the_contract() {
    let parsed = parse(
        "Test Modifier",
        "Another target creature gets -1/-1 until end of turn.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        effect_rule(&parsed)["declaration"]["decisions"][0]["candidates"]["excludeSource"],
        true
    );
}
