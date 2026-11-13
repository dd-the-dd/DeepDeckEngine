use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse_card(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("opponent-library-search-parser-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

fn library_search_rule(parsed: &OracleCardParseResult) -> &Value {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains("searches their library"))
        .and_then(|ability| ability.rule.as_ref())
        .expect("the opponent library-search trigger should emit a rule")
}

#[test]
fn wan_shi_tong_library_search_trigger_is_native_primitive_ir() {
    let parsed = parse_card(
        "Wan Shi Tong, Librarian",
        "Legendary Creature — Bird Spirit",
        "Flash\nFlying, vigilance\nWhen Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down.\nWhenever an opponent searches their library, put a +1/+1 counter on Wan Shi Tong and draw a card.",
    );

    assert_eq!(parsed.status, "canonical");
    let search_ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains("searches their library"))
        .expect("the opponent library-search trigger should be present");
    assert_eq!(search_ability.recognizer, RecognizerProvenance::Native);
    let lexical_name_tokens = search_ability
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert_eq!(lexical_name_tokens, ["Wan", "Shi", "Tong"]);

    let search = library_search_rule(&parsed);
    assert_eq!(
        search["event"],
        json!({
            "kind": "librarySearched",
            "anyPlayer": true,
            "opponentOfSourceController": true,
        })
    );
    assert_eq!(
        search["effects"],
        json!([
            {
                "kind": "putCounters",
                "permanent": { "kind": "self" },
                "counter": "+1/+1",
                "count": { "kind": "integer", "value": 1 },
            },
            {
                "kind": "drawCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "count": { "kind": "integer", "value": 1 },
            },
        ])
    );
    let serialized = serde_json::to_string(&parsed).expect("parse result serializes");
    assert!(!serialized.contains("wanShiTongEnter"));
    assert!(!serialized.contains("wanShiTongSearch"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn sibling_counter_amount_type_source_anaphor_then_and_draw_count_are_generic() {
    let parsed = parse_card(
        "Test Archive",
        "Artifact",
        "Whenever an opponent searches their library, put two -1/-1 counters on this artifact. Then draw two cards.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = parsed
        .abilities
        .first()
        .expect("the sibling trigger should be present");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    let rule = library_search_rule(&parsed);
    assert_eq!(rule["effects"][0]["counter"], "-1/-1");
    assert_eq!(rule["effects"][0]["count"]["value"], 2);
    assert_eq!(rule["effects"][1]["count"]["value"], 2);
}

#[test]
fn a_nonopponent_search_event_is_not_approximated() {
    let parsed = parse_card(
        "Test Archive",
        "Artifact",
        "Whenever a player searches their library, put a charge counter on this artifact and draw a card.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn a_counter_on_the_triggering_player_is_not_rebound_to_the_source() {
    let parsed = parse_card(
        "Test Archive",
        "Artifact",
        "Whenever an opponent searches their library, put a charge counter on that player and draw a card.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn a_different_named_source_is_not_treated_as_self() {
    let parsed = parse_card(
        "Test Archive",
        "Artifact",
        "Whenever an opponent searches their library, put a charge counter on Other Archive and draw a card.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn an_explicit_different_draw_player_is_not_silently_changed_to_controller() {
    let parsed = parse_card(
        "Test Archive",
        "Artifact",
        "Whenever an opponent searches their library, put a charge counter on this artifact and an opponent draws a card.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}
