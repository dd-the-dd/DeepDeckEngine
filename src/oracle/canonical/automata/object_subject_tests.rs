//! Subject-cell shapes of object-subject effect clauses: the attachment
//! status of a plural collection (`Equipped creatures you control`).
use serde_json::Value;

use super::*;
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

fn effect_rule(text: &str) -> Value {
    let parsed = parse("Subject Probe", "Instant", text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_eq!(ability.status, "canonical", "{text}: {parsed:#?}");
    ability.rule.clone().expect("canonical rule")
}

#[test]
fn only_a_plural_noun_makes_the_attachment_adjective_a_status() {
    for (text, collection) in [
        ("Equipped creatures you control gain haste.", true),
        ("Enchanted permanents get +1/+1.", true),
        ("Enchanted creature gets +1/+1.", false),
        ("Equipped creature gains haste.", false),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("test.object-subject", &tokens);
        assert_eq!(
            attached_status_collection_starts(&cursor),
            collection,
            "{text}"
        );
    }
}

#[test]
fn an_attachment_status_collection_receives_a_temporary_grant() {
    // Real card: Resistance Reunited. Sibling, previously unused: a status
    // collection without a controller under a stat modification.
    for text in [
        "Equipped creatures you control gain indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy them.)",
        "Enchanted creatures get +1/+0 and gain first strike until end of turn.",
    ] {
        let rendered = effect_rule(text).to_string();
        assert!(
            rendered.contains("\"isEquipped\"") || rendered.contains("\"hasAttachedAura\""),
            "{text}: {rendered}"
        );
    }
}

#[test]
fn a_status_collection_keeps_the_plural_agreement() {
    // Near miss: a plural collection with a singular verb is not claimed.
    let parsed = parse(
        "Near Miss",
        "Instant",
        "Equipped creatures you control gains indestructible until end of turn.",
    );
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.status != "canonical"),
        "{parsed:#?}"
    );
}
