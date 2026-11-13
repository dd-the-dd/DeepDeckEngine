use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const FATAL_PUSH_ORACLE: &str = "Destroy target creature if it has mana value 2 or less.\nRevolt — Destroy that creature if it has mana value 4 or less instead if a permanent left the battlefield under your control this turn.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Conditional Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn fatal_push_base_effect_is_native_and_checks_the_target_at_resolution() {
    let parsed = parse(FATAL_PUSH_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(ability.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability
        .rule
        .as_ref()
        .expect("base effect emits primitive IR");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
    assert_eq!(
        rule["effects"][0],
        json!({
            "kind": "conditionalEffect",
            "condition": {
                "kind": "objectMatchesFilter",
                "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                "where": {
                    "kind": "compare",
                    "operator": "<=",
                    "left": {
                        "kind": "manaValueOf",
                        "object": { "kind": "candidate" },
                    },
                    "right": { "kind": "integer", "value": 2 },
                },
            },
            "then": [{
                "kind": "destroyPermanent",
                "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
            }],
            "else": [],
        })
    );
}

#[test]
fn revolt_amendment_reuses_the_primary_target_and_event_history_condition() {
    let parsed = parse(FATAL_PUSH_ORACLE);
    let amendment = parsed.abilities[1]
        .rule
        .as_ref()
        .expect("revolt amendment emits primitive IR");

    assert!(amendment.get("declaration").is_none());
    assert_eq!(
        amendment["effects"][0]["condition"],
        json!({
            "kind": "compare",
            "operator": ">=",
            "left": {
                "kind": "countEventsThisTurn",
                "event": "permanentLeftBattlefield",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": null,
            },
            "right": { "kind": "integer", "value": 1 },
        })
    );
    let amended = &amendment["effects"][0]["then"][0];
    assert_eq!(
        amended["condition"]["object"],
        json!({ "kind": "chosenTarget", "id": "targetPermanent" })
    );
    assert_eq!(amended["condition"]["where"]["right"]["value"], 4);
    assert_eq!(amended["then"][0]["kind"], "destroyPermanent");
}

#[test]
fn action_type_amount_and_comparison_direction_are_reusable() {
    let parsed = parse("Exile target artifact if it has mana value 4 or greater.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling emits primitive IR");
    assert_eq!(rule["effects"][0]["then"][0]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][0]["condition"]["where"]["operator"], ">=");
    assert_eq!(
        rule["effects"][0]["condition"]["where"]["right"]["value"],
        4
    );
}

#[test]
fn variable_threshold_declares_the_x_binding_it_lowers() {
    let parsed = parse("Destroy target creature if it has mana value X or less.");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("variable sibling emits a rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        rule["declaration"]["decisions"][0],
        json!({ "id": "xValue", "kind": "chooseNumber", "minimum": 0 })
    );
    assert_eq!(
        rule["effects"][0]["condition"]["where"]["right"],
        json!({ "kind": "decisionResult", "decisionId": "xValue" })
    );
}

#[test]
fn amendment_label_action_noun_and_threshold_direction_are_reusable() {
    let parsed = parse(
        "Renewal — Exile that permanent if it has mana value 7 or greater instead if a permanent left the battlefield under your control this turn.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let amended = &ability.rule.as_ref().unwrap()["effects"][0]["then"][0];
    assert_eq!(amended["condition"]["where"]["operator"], ">=");
    assert_eq!(amended["condition"]["where"]["right"]["value"], 7);
    assert_eq!(amended["then"][0]["kind"], "exilePermanent");
}

#[test]
fn unsupported_comparison_word_is_a_committed_native_failure() {
    let parsed = parse("Destroy target creature if it has mana value 2 or lower.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("conditional-targeted-zone-change")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("condition.bound-object-numeric.direction")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("lower"));
}

#[test]
fn unresolved_anaphor_is_not_silently_accepted() {
    let parsed = parse("Destroy target creature if that has mana value 2 or less.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("condition.bound-object-numeric.anaphor")
    );
}

#[test]
fn unsupported_event_controller_is_a_committed_amendment_failure() {
    let parsed = parse(
        "Revolt — Destroy that creature if it has mana value 4 or less instead if a permanent left the battlefield under an opponent's control this turn.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("conditional-zone-change-amendment")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("condition.permanent-left-battlefield.player")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("an"));
}

#[test]
fn fatal_push_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(FATAL_PUSH_ORACLE);
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.entities.iter())
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
