use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse_keyword_group(
    card_name: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Legendary Creature — Bird Spirit".to_string(),
        mana_cost: Some("{X}{U}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn wan_shi_tong_exact_keyword_group_is_native_primitive_ir() {
    let parsed = parse_keyword_group("Wan Shi Tong, Librarian", "Flying, vigilance");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "keywordAbilityGroup",
            "source": { "kind": "self" },
            "abilities": [
                { "kind": "flying" },
                { "kind": "vigilance" },
            ],
        }))
    );
}

#[test]
fn sibling_keyword_list_reuses_the_same_parameterized_machine() {
    let parsed = parse_keyword_group("Test Keyword Bird", "Flash, flying");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["abilities"],
        json!([{ "kind": "flash" }, { "kind": "flying" }])
    );
}

#[test]
fn singular_keyword_keeps_the_existing_primitive_shape() {
    let parsed = parse_keyword_group("Test Vigilant Bird", "Vigilance");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "vigilance" },
        }))
    );
}

#[test]
fn parameterized_keyword_inside_a_list_is_a_committed_native_failure() {
    let parsed = parse_keyword_group("Test Malformed Bird", "Flying, vigilance 2");

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    assert_eq!(
        parsed.abilities[0].diagnostics[0].code,
        "oracle_machine_failure"
    );
}
