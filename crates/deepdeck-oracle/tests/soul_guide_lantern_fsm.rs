use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const SOUL_GUIDE_LANTERN_ORACLE: &str = "When this artifact enters, exile target card from a graveyard.\n{T}, Sacrifice this artifact: Exile each opponent's graveyard.\n{1}, {T}, Sacrifice this artifact: Draw a card.";

fn parse(name: &str, type_line: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn self_ref() -> Value {
    json!({ "kind": "self" })
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

fn target_graveyard_card() -> Value {
    json!({ "kind": "chosenTarget", "id": "targetGraveyardCard" })
}

fn target_graveyard_cards() -> Value {
    json!({ "kind": "chosenTargets", "id": "targetGraveyardCard" })
}

fn graveyard_target_decision() -> Value {
    json!({
        "kind": "chooseTargets",
        "id": "targetGraveyardCard",
        "minimum": 1,
        "maximum": 1,
        "candidates": {
            "kind": "cards",
            "zone": { "kind": "anyGraveyard" },
            "where": null,
        },
    })
}

fn entry_exile_rule() -> Value {
    let target = target_graveyard_card();
    json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "enterBattlefield", "object": self_ref() },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [graveyard_target_decision()],
        },
        "effects": [{
            "kind": "moveCards",
            "cards": target_graveyard_cards(),
            "to": {
                "kind": "exile",
                "player": { "kind": "ownerOf", "object": target },
                "faceDown": false,
            },
        }],
    })
}

#[test]
fn soul_guide_lantern_is_fully_native_primitive_and_unlinked() {
    let parsed = parse(
        "Soul-Guide Lantern",
        "Artifact",
        "{1}",
        SOUL_GUIDE_LANTERN_ORACLE,
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("native rule"))
        .collect::<Vec<_>>();
    assert_eq!(
        rules,
        vec![
            entry_exile_rule(),
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "tap", "object": self_ref() },
                    { "kind": "sacrificePermanent", "permanent": self_ref() },
                ],
                "declaration": { "kind": "castingDeclaration", "decisions": [] },
                "effects": [{
                    "kind": "exileAllGraveyards",
                    "exceptPlayer": controller(),
                }],
            }),
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}" },
                    { "kind": "tap", "object": self_ref() },
                    { "kind": "sacrificePermanent", "permanent": self_ref() },
                ],
                "declaration": { "kind": "castingDeclaration", "decisions": [] },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": { "kind": "integer", "value": 1 },
                }],
            }),
        ]
    );

    let serialized = serde_json::to_string(&rules).expect("rules serialize");
    assert!(!serialized.contains("exileTargetCardWithSource"));
    assert!(!serialized.contains("exiledWithSource"));
    assert!(!serialized.contains("linkId"));
}

#[test]
fn source_entry_targeted_graveyard_exile_generalizes_across_source_kinds_and_articles() {
    for text in [
        "When this creature enters, exile target card from a graveyard.",
        "When this enchantment enters, exile target card from any graveyard.",
    ] {
        let parsed = parse("Test Relic", "Artifact Creature", "{2}", text);
        assert_eq!(parsed.status, "canonical", "{text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(parsed.abilities[0].rule.as_ref(), Some(&entry_exile_rule()));
    }
}

#[test]
fn activated_zone_collection_generalizes_all_vs_opponents() {
    let opponents = parse(
        "Test Relic",
        "Artifact",
        "{1}",
        "{T}, Sacrifice this artifact: Exile each opponent's graveyard.",
    );
    let all = parse(
        "Test Vessel",
        "Artifact",
        "{1}",
        "{1}, Sacrifice this artifact: Exile all graveyards.",
    );

    assert_eq!(
        opponents.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(all.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        opponents.abilities[0].rule.as_ref().unwrap()["effects"][0],
        json!({ "kind": "exileAllGraveyards", "exceptPlayer": controller() })
    );
    assert_eq!(
        all.abilities[0].rule.as_ref().unwrap()["effects"][0],
        json!({ "kind": "exileAllGraveyards" })
    );
}

#[test]
fn activated_targetless_draw_reuses_arbitrary_supported_cost_sequences() {
    let parsed = parse(
        "Test Bauble",
        "Artifact",
        "{0}",
        "{2}, {T}, Sacrifice this artifact: Draw a card.",
    );
    let rule = parsed.abilities[0].rule.as_ref().expect("native draw rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["costs"].as_array().unwrap().len(), 3);
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
}

#[test]
fn entry_card_zone_change_rejects_an_unsupported_source_zone_at_a_precise_state() {
    let parsed = parse(
        "Test Relic",
        "Artifact",
        "{1}",
        "When this artifact enters, exile target card from the graveyard.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    // Both the shared trigger program and the entry family reject the
    // unowned `the graveyard` in the same shared card-target child.
    for automaton in ["generic-trigger-program", "source-entry-card-zone-change"] {
        assert!(
            ability.diagnostics.iter().any(|diagnostic| {
                diagnostic.automaton.as_deref() == Some(automaton)
                    && diagnostic.state.as_deref() == Some("card-target.source-owner")
                    && diagnostic.found.as_deref() == Some("the")
            }),
            "{automaton}: {:#?}",
            ability.diagnostics
        );
    }
}

#[test]
fn opponent_graveyard_collection_rejects_a_different_zone_without_fallback() {
    let parsed = parse(
        "Test Relic",
        "Artifact",
        "{1}",
        "{T}, Sacrifice this artifact: Exile each opponent's hand.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.zone-batch.collection")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("hand"));
}

#[test]
fn activated_targetless_family_rejects_trailing_tokens_with_full_consumption() {
    let parsed = parse(
        "Test Bauble",
        "Artifact",
        "{0}",
        "{T}, Sacrifice this artifact: Draw a card flying.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert!(ability.rule.is_none());
}
