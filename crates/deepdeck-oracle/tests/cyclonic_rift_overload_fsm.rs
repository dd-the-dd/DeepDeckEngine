use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const REMINDER: &str = "(You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")";

fn parse_instant(name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn rule(parsed: &OracleCardParseResult, index: usize) -> &Value {
    parsed.abilities[index]
        .rule
        .as_ref()
        .expect("native ability emits a rule")
}

#[test]
fn cyclonic_rift_compiles_overload_into_normal_and_each_branches() {
    let parsed = parse_instant(
        "Cyclonic Rift",
        "{1}{U}",
        &format!(
            "Return target nonland permanent you don't control to its owner's hand.\nOverload {{6}}{{U}} {REMINDER}"
        ),
    );

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
    let unknowns = parsed
        .abilities
        .iter()
        .flat_map(|ability| &ability.entities)
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknowns.is_empty(), "unresolved tokens: {unknowns:?}");

    let spell = rule(&parsed, 0);
    let decision = &spell["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(
        decision["candidates"]["controller"],
        json!({
            "kind": "playersExcept",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        })
    );
    assert_eq!(
        decision["condition"],
        json!({
            "kind": "not",
            "operand": { "kind": "wasCastWithAlternativeCost", "mode": "overload" },
        })
    );
    assert_eq!(
        decision["candidates"]["where"],
        json!({
            "kind": "not",
            "operand": { "kind": "cardTypeContains", "value": "Land" },
        })
    );

    let branch = &spell["effects"][0];
    assert_eq!(branch["kind"], "conditionalEffect");
    assert_eq!(
        branch["condition"],
        json!({ "kind": "wasCastWithAlternativeCost", "mode": "overload" })
    );
    assert_eq!(branch["then"][0]["kind"], "returnToOwnersHand");
    assert_eq!(branch["then"][0]["object"]["kind"], "eachPermanent");
    assert_eq!(branch["then"][0]["object"]["excludeSourceController"], true);
    assert_eq!(branch["else"][0]["kind"], "returnToOwnersHand");
    assert_eq!(branch["else"][0]["object"]["kind"], "chosenTarget");

    let overload = rule(&parsed, 1);
    assert_eq!(overload["kind"], "keywordAbility");
    assert_eq!(overload["ability"]["kind"], "alternativeCost");
    assert_eq!(overload["ability"]["mode"], "overload");
    assert_eq!(overload["ability"]["costs"][0]["kind"], "payMana");
    assert_eq!(overload["ability"]["costs"][0]["manaCost"], "{6}{U}");
    assert!(overload["ability"].get("compileDirective").is_none());
    assert!(!spell.to_string().contains("returnPermanentsToOwnersHands"));
}

#[test]
fn sibling_pattern_reuses_negative_controller_scope_and_target_to_each_composition() {
    let parsed = parse_instant(
        "Sibling",
        "{R}",
        &format!("Destroy target artifact you don't control.\nOverload {{4}}{{R}} {REMINDER}"),
    );

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("serialize parser diagnostics")
    );
    let spell = rule(&parsed, 0);
    assert_eq!(
        spell["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "playersExcept"
    );
    assert_eq!(
        spell["effects"][0]["then"][0]["permanent"]["kind"],
        "eachPermanent"
    );
    assert_eq!(
        spell["effects"][0]["then"][0]["permanent"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
}

#[test]
fn unfiltered_permanent_selector_does_not_emit_a_null_collection_filter() {
    let parsed = parse_instant(
        "Unfiltered Sibling",
        "{1}{U}",
        &format!(
            "Return target permanent you don't control to its owner's hand.\nOverload {{4}}{{U}} {REMINDER}"
        ),
    );

    assert_eq!(parsed.status, "canonical");
    let collection = &rule(&parsed, 0)["effects"][0]["then"][0]["object"];
    assert_eq!(collection["kind"], "eachPermanent");
    assert!(collection.get("where").is_none());
}

#[test]
fn failed_target_domain_composition_leaves_the_normal_spell_rule_unchanged() {
    let parsed = parse_instant(
        "Unsupported Domain",
        "{1}{U}",
        &format!("Counter target spell.\nOverload {{4}}{{U}} {REMINDER}"),
    );

    assert_eq!(parsed.status, "unsupported");
    let counter = rule(&parsed, 0);
    let decision = &counter["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert!(decision.get("condition").is_none());
    assert_eq!(counter["effects"][0]["kind"], "counterSpell");
    assert_eq!(counter["effects"][0]["spell"]["kind"], "chosenTarget");

    let overload = &parsed.abilities[1];
    assert_eq!(overload.recognizer, RecognizerProvenance::Native);
    assert_eq!(overload.status, "unsupported");
    assert_eq!(
        overload.diagnostics[0].state.as_deref(),
        Some("composition.selector-domain")
    );
}

#[test]
fn negative_controller_relation_rejects_a_wrong_relation_locally() {
    let parsed = parse_instant(
        "Near Miss",
        "{1}{U}",
        "Return target nonland permanent you don't own to its owner's hand.",
    );
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.controller-qualifier.negated-relation")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("own"));
    assert_eq!(ability.diagnostics[0].expected.as_deref(), Some("control"));
}

#[test]
fn overload_reminder_near_miss_is_a_committed_native_failure() {
    let parsed = parse_instant(
        "Reminder Near Miss",
        "{1}{U}",
        "Overload {6}{U} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"every.\")",
    );
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("keyword-overload")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("keyword.overload.reminder.quoted-each")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("every"));
}
