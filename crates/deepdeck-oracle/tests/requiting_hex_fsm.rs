use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const REQUITTING_HEX_ORACLE: &str = "As an additional cost to cast this spell, you may blight 1. (You may put a -1/-1 counter on a creature you control.)\nDestroy target creature with mana value 2 or less. If this spell's additional cost was paid, you gain 2 life.";

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Requiting Hex".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn requiting_hex_is_native_and_composes_primitive_cost_and_effects() {
    let parsed = parse_instant(REQUITTING_HEX_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );

    let cost_rule = parsed.abilities[0].rule.as_ref().expect("cost rule");
    assert_eq!(
        cost_rule["declaration"]["decisions"][0]["options"],
        json!(["decline", "pay"])
    );
    assert_eq!(
        cost_rule["declaration"]["decisions"][1]["condition"],
        json!({
            "kind": "selectionContains",
            "selection": { "kind": "decisionResult", "decisionId": "additionalCostMode" },
            "value": "pay",
        })
    );
    assert_eq!(
        cost_rule["declaration"]["additionalCosts"][0]["then"][0],
        json!({
            "kind": "putCounters",
            "permanent": { "kind": "chosenTarget", "id": "blightCost1" },
            "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
            "counter": "-1/-1",
            "count": { "kind": "integer", "value": 1 },
        })
    );

    let effect_rule = parsed.abilities[1].rule.as_ref().expect("effect rule");
    assert_eq!(
        effect_rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Creature" },
                {
                    "kind": "compare",
                    "operator": "<=",
                    "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                    "right": { "kind": "integer", "value": 2 },
                },
            ],
        })
    );
    assert_eq!(effect_rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(effect_rule["effects"][1]["kind"], "conditionalEffect");
    assert_eq!(effect_rule["effects"][1]["then"][0]["kind"], "gainLife");
    assert!(
        !effect_rule
            .to_string()
            .contains("resolveTriggeredInstruction")
    );
}

#[test]
fn sibling_values_and_permanent_types_reuse_the_same_automata() {
    let parsed = parse_instant(
        "As an additional cost to cast this spell, you may blight 2. (You may put two -1/-1 counters on a creature you control.)\nDestroy target artifact with mana value 4 or less. If this spell's additional cost was paid, you gain 5 life.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["declaration"]["additionalCosts"][0]["then"][0]
            ["count"]["value"],
        2
    );
    let effect = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(
        effect["declaration"]["decisions"][0]["candidates"]["where"]["operands"][0],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        effect["declaration"]["decisions"][0]["candidates"]["where"]["operands"][1]["right"]["value"],
        4
    );
    assert_eq!(effect["effects"][1]["then"][0]["amount"]["value"], 5);
}

#[test]
fn greater_than_qualifier_reuses_the_shared_numeric_selector() {
    let parsed = parse_instant(
        "Destroy target artifact with mana value 4 or greater. If this spell's additional cost was paid, you gain 5 life.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let filter = &parsed.abilities[0].rule.as_ref().unwrap()["declaration"]["decisions"][0]["candidates"]
        ["where"]["operands"][1];
    assert_eq!(filter["kind"], "compare");
    assert_eq!(filter["operator"], ">=");
    assert_eq!(filter["right"]["value"], 4);
}

#[test]
fn promoted_families_surface_precise_near_miss_states_without_fallback() {
    for (oracle_text, automaton, state, found) in [
        (
            "As an additional cost to cast this spell, you may blight 2. (You may put one -1/-1 counter on a creature you control.)",
            "additional-cost",
            "additional-cost.reminder",
            Some("("),
        ),
        (
            "Destroy target creature with mana value 2 or lower. If this spell's additional cost was paid, you gain 2 life.",
            "additional-cost-paid-followup",
            "selector.numeric-comparison.direction",
            Some("lower"),
        ),
        (
            "Destroy target creature with mana value 2 or less. If this spell's additional cost was ignored, you gain 2 life.",
            "additional-cost-paid-followup",
            "condition.additional-cost-paid.if",
            Some("ignored"),
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_ne!(ability.status, "canonical", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), found);
    }
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_instant(REQUITTING_HEX_ORACLE);
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.entities.iter())
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
