use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const GLOOMLAKE_VERGE_ORACLE: &str =
    "{T}: Add {U}.\n{T}: Add {B}. Activate only if you control an Island or a Swamp.";

fn parse_land(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn fixed_mana_rule(mana: &str, activation_condition: Option<Value>) -> Value {
    let mut rule = json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "addMana",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "mana": mana,
        }],
    });
    if let Some(condition) = activation_condition {
        rule["activationCondition"] = condition;
    }
    rule
}

fn controls_where(where_clause: Value) -> Value {
    json!({
        "kind": "controlsPermanent",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "where": where_clause,
    })
}

#[test]
fn gloomlake_verge_is_two_native_primitive_mana_abilities() {
    let parsed = parse_land("Gloomlake Verge", GLOOMLAKE_VERGE_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical"
            && ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&fixed_mana_rule("{U}", None))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&fixed_mana_rule(
            "{B}",
            Some(controls_where(json!({
                "kind": "or",
                "operands": [
                    { "kind": "subtypeContains", "value": "Island" },
                    { "kind": "subtypeContains", "value": "Swamp" },
                ],
            }))),
        ))
    );
}

#[test]
fn controlled_permanent_presence_condition_generalizes_across_output_and_predicates() {
    let tainted = parse_land(
        "Tainted Test Land",
        "{T}: Add {U} or {B}. Activate only if you control a Swamp.",
    );
    assert_eq!(tainted.status, "canonical");
    assert_eq!(
        tainted.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    let tainted_rule = tainted.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        tainted_rule["effects"][0]["mana"],
        json!({ "kind": "chooseOne", "options": ["{U}", "{B}"] })
    );
    assert_eq!(
        tainted_rule["activationCondition"],
        controls_where(json!({ "kind": "subtypeContains", "value": "Swamp" }))
    );

    let typed_union = parse_land(
        "Conditional Test Land",
        "{T}: Add {G}. Activate only if you control an artifact or an enchantment.",
    );
    assert_eq!(typed_union.status, "canonical");
    assert_eq!(
        typed_union.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        typed_union.abilities[0].rule.as_ref().unwrap()["activationCondition"],
        controls_where(json!({
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Artifact" },
                { "kind": "cardTypeContains", "value": "Enchantment" },
            ],
        }))
    );
}

#[test]
fn malformed_condition_is_a_committed_native_failure_at_the_condition_child() {
    let parsed = parse_land(
        "Broken Conditional Land",
        "{T}: Add {B}. Activate only if you control an Island or.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("condition.controls-permanent.branch-determiner")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("."));
    assert_eq!(
        ability.diagnostics[0].expected.as_deref(),
        Some("an indefinite object determiner")
    );
}
