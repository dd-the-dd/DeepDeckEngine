use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const RHYSTIC_ORACLE: &str =
    "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Rhystic Study".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn opponent_spell_taxed_optional_draw_is_native_and_primitive() {
    let parsed = parse(RHYSTIC_ORACLE);
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
                "where": null,
            },
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": { "kind": "triggeringPlayer" },
                "cost": { "kind": "payMana", "manaCost": "{1}" },
                "effects": [],
                "declineEffects": [{
                    "kind": "optionalEffects",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "effects": [{
                        "kind": "drawCards",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "count": { "kind": "integer", "value": 1 },
                    }],
                }],
            }],
        }))
    );
    let serialized = ability.rule.as_ref().expect("primitive rule").to_string();
    assert!(!serialized.contains("resolveTriggeredInstruction"));
    assert!(!serialized.contains("operation"));
}

#[test]
fn draw_count_and_tax_cost_are_owned_by_shared_children() {
    let parsed = parse(
        "Whenever an opponent casts a spell, you may draw two cards unless that player pays {2}.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling primitive rule");
    assert_eq!(rule["effects"][0]["cost"]["manaCost"], "{2}");
    assert_eq!(
        rule["effects"][0]["declineEffects"][0]["effects"][0]["count"],
        json!({ "kind": "integer", "value": 2 })
    );
}

#[test]
fn any_player_event_reuses_the_same_event_binding_and_tax_effect() {
    let parsed =
        parse("Whenever a player casts a spell, you may draw a card unless that player pays {3}.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("any-player sibling rule");
    assert_eq!(
        rule["event"],
        json!({ "kind": "spellCast", "anyPlayer": true, "where": null })
    );
    assert_eq!(
        rule["effects"][0]["player"],
        json!({ "kind": "triggeringPlayer" })
    );
    assert_eq!(rule["effects"][0]["cost"]["manaCost"], "{3}");
}

#[test]
fn truncated_unless_and_unbound_player_anaphor_are_committed_failures() {
    for (text, state, found) in [
        (
            "Whenever an opponent casts a spell, you may draw a card unless that player pays.",
            "cost.mana.start",
            ".",
        ),
        (
            "Whenever an opponent casts a spell, you may draw a card unless player pays {1}.",
            "reference.triggering-player.determiner",
            "player",
        ),
    ] {
        let parsed = parse(text);
        let ability = &parsed.abilities[0];

        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert!(ability.rule.is_none());
        assert_family_failure(ability, state, found);
    }
}

/// The shared trigger program reports its own committed failure first; the
/// tax-draw family's precise near-miss state stays visible as evidence.
fn assert_family_failure(
    ability: &deepdeck_oracle::oracle::ParsedOracleAbility,
    state: &str,
    found: &str,
) {
    let diagnostics = &ability.diagnostics;
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.automaton.as_deref() == Some("player-spell-tax-draw")
                && diagnostic.state.as_deref() == Some(state)
                && diagnostic.found.as_deref() == Some(found)
        }),
        "{state}: {diagnostics:#?}"
    );
}

#[test]
fn trailing_sentence_fails_full_consumption() {
    let parsed = parse(
        "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}. Draw a card.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_family_failure(ability, "trigger.player-spell-tax-draw.complete", "Draw");
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(RHYSTIC_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
