use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const BIRDS_OF_PARADISE_ORACLE: &str = "Flying\n{T}: Add one mana of any color.";

fn parse_card(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Creature — Bird".to_string(),
        mana_cost: Some("{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn birds_of_paradise_has_two_independent_native_primitive_abilities() {
    let parsed = parse_card("Birds of Paradise", BIRDS_OF_PARADISE_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical"
            && ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "flying" },
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule,
        Some(json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "mana": { "kind": "chooseColor", "amount": 1 },
            }],
        }))
    );
}

#[test]
fn flying_and_fixed_mana_sibling_reuses_both_native_children() {
    let parsed = parse_card("Test Flying Mana Bird", "Flying\n{T}: Add {U}.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
        "flying"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["kind"],
        "addMana"
    );
}

#[test]
fn parameterized_flying_near_miss_is_not_claimed_as_a_simple_keyword() {
    let parsed = parse_card("Test Unsupported Flying", "Flying 2");

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    assert_ne!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
}
