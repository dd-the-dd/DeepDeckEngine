use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ORACLE: &str = "When you play another land, sacrifice this land.\n{T}: Add {C}{C}.";

fn parse_land(name: &str, oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn city_of_traitors_is_fully_native_with_primitive_land_event_and_sacrifice() {
    let parsed = parse_land("City of Traitors", ORACLE);

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("serialize parser diagnostics")
    );
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let trigger = parsed.abilities[0].rule.as_ref().expect("trigger rule");
    assert_eq!(
        trigger,
        &json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": {
                "kind": "landPlayed",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "excludeSource": true,
            },
            "effects": [{
                "kind": "sacrificePermanent",
                "permanent": { "kind": "self" },
            }],
        })
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().expect("mana rule")["effects"][0]["kind"],
        "addMana"
    );
    let unknowns = parsed
        .abilities
        .iter()
        .flat_map(|ability| &ability.entities)
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknowns.is_empty(), "unresolved tokens: {unknowns:?}");
}

#[test]
fn siblings_reuse_named_and_typed_source_references_and_both_trigger_words() {
    let named = parse_land(
        "Patient City",
        "Whenever you play another land, sacrifice Patient City.",
    );
    assert_eq!(named.status, "canonical");
    assert_eq!(named.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        named.abilities[0].rule.as_ref().expect("named rule")["effects"][0]["permanent"],
        json!({ "kind": "self" })
    );

    let typed = parse_land(
        "Patient Relic",
        "Whenever you play another land, sacrifice this artifact.",
    );
    assert_eq!(typed.status, "canonical");
    assert_eq!(typed.abilities[0].recognizer, RecognizerProvenance::Native);

    let draw = parse_land(
        "Patient Archive",
        "When you play another land, draw a card.",
    );
    assert_eq!(draw.status, "canonical");
    assert_eq!(draw.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        draw.abilities[0].rule.as_ref().expect("draw rule")["effects"][0]["kind"],
        "drawCards"
    );
}

#[test]
fn indefinite_land_is_not_source_excluding_and_other_types_fail_locally() {
    let indefinite = parse_land(
        "Patient City",
        "Whenever you play a land, sacrifice this land.",
    );
    assert_eq!(indefinite.status, "canonical");
    assert_eq!(
        indefinite.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert!(
        indefinite.abilities[0]
            .rule
            .as_ref()
            .expect("indefinite rule")["event"]
            .get("excludeSource")
            .is_none()
    );

    let wrong_type = parse_land(
        "Patient City",
        "When you play another creature, sacrifice this land.",
    );
    let ability = &wrong_type.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("event.land-played.object-type")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("creature"));

    let wrong_agreement = parse_land(
        "Patient City",
        "When you plays another land, sacrifice this land.",
    );
    let ability = &wrong_agreement.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    // The trigger frame tries every player-event family and reports the one
    // that read furthest; the player-action families (`play a land`, `draw
    // your Nth card`) all stop at the same verb, so the diagnostic is an
    // event action slot at the misconjugated verb, whichever family won.
    // The attack declaration names that slot `verb` (`you attack with`).
    let state = ability.diagnostics[0].state.as_deref().unwrap_or_default();
    assert!(
        state.starts_with("event.") && (state.ends_with(".action") || state.ends_with(".verb")),
        "{state}"
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("plays"));
}
