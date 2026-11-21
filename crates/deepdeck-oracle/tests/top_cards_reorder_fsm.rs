use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Look at the top three cards of your library, then put them back in any order. You may shuffle.\nDraw a card.";

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Top Reorder".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn exact_reorder_optional_shuffle_and_draw_are_native_primitive_rules() {
    let parsed = parse_spell(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    let player = controller();
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [
                {
                    "kind": "lookAtTopCards",
                    "zone": { "kind": "library", "player": player.clone() },
                    "count": { "kind": "integer", "value": 3 },
                    "bind": "lookedCards",
                },
                {
                    "kind": "chooseOrder",
                    "id": "topOrder",
                    "player": player.clone(),
                    "objects": { "kind": "boundObjects", "binding": "lookedCards" },
                },
                {
                    "kind": "moveCards",
                    "cards": { "kind": "decisionResult", "decisionId": "topOrder" },
                    "to": {
                        "kind": "library",
                        "player": player.clone(),
                        "position": "top",
                    },
                    "order": { "kind": "decisionOrder", "decisionId": "topOrder" },
                },
                {
                    "kind": "optionalEffects",
                    "player": player.clone(),
                    "effects": [{
                        "kind": "shuffleZone",
                        "zone": { "kind": "library", "player": player.clone() },
                    }],
                },
            ],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "drawCards",
                "player": player,
                "count": { "kind": "integer", "value": 1 },
            }],
        }))
    );
}

#[test]
fn singular_collection_reference_and_draw_count_are_generic_children() {
    let parsed = parse_spell(
        "Look at the top one card of your library, then put it back in any order. You may shuffle.\nDraw three cards.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["count"],
        json!({ "kind": "integer", "value": 1 })
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["count"],
        json!({ "kind": "integer", "value": 3 })
    );
}

#[test]
fn claimed_near_misses_fail_at_the_binding_or_modality_state() {
    for (oracle_text, state, found) in [
        (
            "Look at the top three cards of your library, then put it back in any order. You may shuffle.",
            "reference.collection.bound",
            "it",
        ),
        (
            "Look at the top three cards of your library, then put them back in any order. You shuffle.",
            "effect.optional-shuffle.modality",
            "shuffle",
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some("top-cards-reorder"));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
