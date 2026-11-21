use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};
use serde_json::Value;

fn parse_search_rule(card_name: &str, type_line: &str, oracle_text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("opponent-library-search-contract-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract test thread starts")
        .join()
        .expect("contract test thread completes");
    assert_eq!(parsed.status, "canonical");
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains("searches their library"))
        .expect("the opponent library-search trigger should be present");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    ability
        .rule
        .clone()
        .expect("the opponent library-search trigger should emit a rule")
}

#[test]
fn wan_shi_tong_search_trigger_is_primitive_and_executable() {
    let rule = parse_search_rule(
        "Wan Shi Tong, Librarian",
        "Legendary Creature — Bird Spirit",
        "Flash\nFlying, vigilance\nWhen Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down.\nWhenever an opponent searches their library, put a +1/+1 counter on Wan Shi Tong and draw a card.",
    );

    assert!(rule_is_executable(&rule));
    let serialized = rule.to_string();
    assert!(!serialized.contains("resolveTriggeredInstruction"));
    assert!(!serialized.contains("wanShiTongSearch"));
}

#[test]
fn counter_and_draw_sibling_uses_the_same_executable_primitive_shape() {
    let rule = parse_search_rule(
        "Test Archive",
        "Artifact",
        "Whenever an opponent searches their library, put two -1/-1 counters on this artifact. Then draw two cards.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"][0]["kind"], "putCounters");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
}
