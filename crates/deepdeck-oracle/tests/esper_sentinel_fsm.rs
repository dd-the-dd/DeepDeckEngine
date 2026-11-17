use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Whenever an opponent casts their first noncreature spell each turn, draw a card unless that player pays {X}, where X is this creature's power.";

fn parse(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn exact_esper_sentinel_is_native_primitive_ir() {
    let parsed = parse(
        "Esper Sentinel",
        "Artifact Creature — Human Soldier",
        EXACT_ORACLE,
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": {
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": {
                    "kind": "not",
                    "operand": { "kind": "cardTypeContains", "value": "Creature" },
                },
                "spellCastOrdinal": { "kind": "integer", "value": 1 },
            },
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": { "kind": "triggeringPlayer" },
                "cost": {
                    "kind": "payMana",
                    "genericAmount": {
                        "kind": "powerOf",
                        "object": { "kind": "self" },
                    },
                },
                "effects": [],
                "declineEffects": [{
                    "kind": "drawCards",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "count": { "kind": "integer", "value": 1 },
                }],
            }],
        }))
    );
    let serialized = ability.rule.as_ref().expect("primitive rule").to_string();
    assert!(!serialized.contains("resolveTriggeredInstruction"));
    assert!(!serialized.contains("esperSentinel"));
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown"),
        "unexpected unknowns: {:#?}",
        ability.entities
    );
}

#[test]
fn sibling_varies_ordinal_filter_draw_count_and_source_property() {
    let parsed = parse(
        "Generic Ordinal Tax Test",
        "Artifact",
        "Whenever an opponent casts their second instant or sorcery spell each turn, draw two cards unless that player pays {X}, where X is this permanent's toughness.",
    );
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("sibling primitive rule");

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["event"]["spellCastOrdinal"]["value"], 2);
    assert_eq!(rule["event"]["where"]["kind"], "or");
    assert_eq!(rule["event"]["where"]["operands"][0]["value"], "Instant");
    assert_eq!(rule["event"]["where"]["operands"][1]["value"], "Sorcery");
    assert_eq!(
        rule["effects"][0]["cost"]["genericAmount"]["kind"],
        "toughnessOf"
    );
    assert_eq!(rule["effects"][0]["declineEffects"][0]["count"]["value"], 2);
}

#[test]
fn unsupported_period_is_committed_to_native_diagnostic() {
    let parsed = parse(
        "Malformed Ordinal Tax Test",
        "Artifact Creature — Construct",
        "Whenever an opponent casts their first noncreature spell each game, draw a card unless that player pays {X}, where X is this creature's power.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("event.spell-cast.ordinal-period")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("game"));
}
