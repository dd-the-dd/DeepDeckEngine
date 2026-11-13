use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const SMOTHERING_TITHE_ORACLE: &str = "Whenever an opponent draws a card, that player may pay {2}. If the player doesn't, you create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Smothering Tithe".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{3}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn opponent_draw_payment_decline_is_native_and_primitive() {
    let parsed = parse(SMOTHERING_TITHE_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": {
                "kind": "cardDrawn",
                "opponentOfSourceController": true,
            },
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": { "kind": "triggeringPlayer" },
                "cost": { "kind": "payMana", "manaCost": "{2}" },
                "effects": [],
                "declineEffects": [{
                    "kind": "createTokens",
                    "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": { "kind": "namedToken", "name": "Treasure" },
                }],
            }],
        }))
    );
    let serialized = ability.rule.as_ref().expect("primitive rule").to_string();
    assert!(!serialized.contains("opponentMayPayManaOrCreateToken"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn payment_cost_and_named_token_are_reusable_children() {
    let parsed = parse(
        "Whenever an opponent draws a card, that player may pay {1}. If the player doesn't, you create a Clue token.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling primitive rule");
    assert_eq!(rule["effects"][0]["cost"]["manaCost"], "{1}");
    assert_eq!(
        rule["effects"][0]["declineEffects"][0]["token"],
        json!({ "kind": "namedToken", "name": "Clue" })
    );
}

#[test]
fn malformed_bound_player_and_negative_predicate_fail_at_owned_states() {
    for (oracle, state, found) in [
        (
            "Whenever an opponent draws a card, that player may pay {2}. If the opponent doesn't, you create a Treasure token.",
            "reference.triggering-player.definite-noun",
            "opponent",
        ),
        (
            "Whenever an opponent draws a card, that player may pay {2}. If the player does, you create a Treasure token.",
            "effect.event-bound-payment.negative-predicate",
            "does",
        ),
    ] {
        let parsed = parse(oracle);
        let ability = &parsed.abilities[0];

        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert!(ability.rule.is_none());
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(SMOTHERING_TITHE_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
