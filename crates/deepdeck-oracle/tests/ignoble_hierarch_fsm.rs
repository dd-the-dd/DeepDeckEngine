use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse_creature(
    card_name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn ignoble_hierarch_exact_oracle_is_entirely_native() {
    let parsed = parse_creature(
        "Ignoble Hierarch",
        "Creature — Goblin Shaman",
        "{G}",
        "Exalted (Whenever a creature you control attacks alone, that creature gets +1/+1 until end of turn.)\n{T}: Add {B}, {R}, or {G}.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));
    assert!(
        parsed.abilities[0]
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "exalted" },
        }))
    );
}

#[test]
fn another_parameterless_keyword_reuses_the_same_machine() {
    let parsed = parse_creature("Wind Drake", "Creature — Drake", "{2}{U}", "Flying");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "flying" },
        }))
    );
}

#[test]
fn exalted_without_reminder_keeps_the_same_primitive_shape() {
    let parsed = parse_creature(
        "Reminder-Free Exalted Creature",
        "Creature — Human Knight",
        "{1}{W}",
        "Exalted",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "exalted" },
        }))
    );
}

#[test]
fn parameterized_exalted_is_a_committed_native_failure() {
    let parsed = parse_creature(
        "Malformed Exalted Creature",
        "Creature — Human",
        "{1}{G}",
        "Exalted 2",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    assert_eq!(
        parsed.abilities[0].diagnostics[0].code,
        "oracle_machine_failure"
    );
}

#[test]
fn exalted_reminder_with_the_wrong_modifier_is_rejected_natively() {
    let parsed = parse_creature(
        "Malformed Exalted Reminder Creature",
        "Creature — Human Knight",
        "{1}{W}",
        "Exalted (Whenever a creature you control attacks alone, that creature gets +2/+2 until end of turn.)",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    assert_eq!(
        parsed.abilities[0].diagnostics[0].code,
        "oracle_machine_failure"
    );
}
