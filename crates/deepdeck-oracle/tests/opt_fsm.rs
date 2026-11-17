use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const OPT_ORACLE: &str = "Scry 1. (Look at the top card of your library. You may put that card on the bottom.)\nDraw a card.";

fn parse_spell(name: &str, mana_cost: &str, oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn contains_effect_kind(value: &Value, kind: &str) -> bool {
    match value {
        Value::Array(values) => values.iter().any(|value| contains_effect_kind(value, kind)),
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) == Some(kind)
                || object
                    .values()
                    .any(|value| contains_effect_kind(value, kind))
        }
        _ => false,
    }
}

#[test]
fn opt_is_fully_native_and_keeps_draw_separate_from_atomic_scry() {
    let parsed = parse_spell("Opt", "{U}", OPT_ORACLE);

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("serialize diagnostics")
    );
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(
        parsed
            .abilities
            .iter()
            .flat_map(|ability| &ability.entities)
            .all(|entity| entity.token_type != "unknown")
    );

    let scry = parsed.abilities[0].rule.as_ref().expect("scry rule");
    assert!(!contains_effect_kind(scry, "scry"));
    assert_eq!(
        scry,
        &json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [
                {
                    "kind": "lookAtTopCards",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } }
                    },
                    "count": { "kind": "integer", "value": 1 },
                    "bind": "scryLookedCards"
                },
                {
                    "kind": "chooseCards",
                    "id": "scryBottomCards",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "from": { "kind": "boundObjects", "binding": "scryLookedCards" },
                    "minimum": { "kind": "integer", "value": 0 },
                    "maximum": {
                        "kind": "countObjects",
                        "objects": { "kind": "boundObjects", "binding": "scryLookedCards" }
                    }
                },
                {
                    "kind": "chooseOrder",
                    "id": "scryBottomOrder",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "objects": { "kind": "decisionResult", "decisionId": "scryBottomCards" }
                },
                {
                    "kind": "moveCards",
                    "cards": { "kind": "decisionResult", "decisionId": "scryBottomOrder" },
                    "to": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "position": "bottom"
                    },
                    "order": { "kind": "decisionOrder", "decisionId": "scryBottomOrder" }
                },
                {
                    "kind": "chooseOrder",
                    "id": "scryTopOrder",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "objects": {
                        "kind": "setDifference",
                        "left": { "kind": "boundObjects", "binding": "scryLookedCards" },
                        "right": { "kind": "decisionResult", "decisionId": "scryBottomCards" }
                    }
                },
                {
                    "kind": "moveCards",
                    "cards": { "kind": "decisionResult", "decisionId": "scryTopOrder" },
                    "to": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "position": "top"
                    },
                    "order": { "kind": "decisionOrder", "decisionId": "scryTopOrder" }
                },
                {
                    "kind": "emitAbilitySignal",
                    "abilityId": "scry",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } }
                }
            ]
        })
    );

    let draw = parsed.abilities[1].rule.as_ref().expect("draw rule");
    assert_eq!(draw["effects"].as_array().expect("draw effects").len(), 1);
    assert_eq!(draw["effects"][0]["kind"], "drawCards");
}

#[test]
fn sibling_quantity_reuses_the_same_scry_primitive_graph() {
    let parsed = parse_spell("Deeper Opt", "{1}{U}", "Scry 2.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("scry rule");
    assert_eq!(rule["effects"][0]["kind"], "lookAtTopCards");
    assert_eq!(
        rule["effects"][0]["count"],
        json!({ "kind": "integer", "value": 2 })
    );
    assert_eq!(rule["effects"][6]["kind"], "emitAbilitySignal");
    assert!(!contains_effect_kind(rule, "scry"));
}

#[test]
fn scry_zero_executes_the_empty_partition_without_emitting_a_scry_signal() {
    let parsed = parse_spell("Still Opt", "{U}", "Scry 0.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("scry zero rule");
    assert_eq!(rule["effects"].as_array().expect("effects").len(), 6);
    assert_eq!(
        rule["effects"][0]["count"],
        json!({ "kind": "integer", "value": 0 })
    );
    assert!(!contains_effect_kind(rule, "emitAbilitySignal"));
}

#[test]
fn reminder_with_a_graveyard_destination_fails_at_the_committed_scry_state() {
    let parsed = parse_spell(
        "False Opt",
        "{U}",
        "Scry 1. (Look at the top card of your library. You may put that card in your graveyard.)",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.scry.reminder.destination")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("in"));
}
