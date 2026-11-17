use super::*;

fn empower(count: NumericExpression) -> EmpowerJaceEffectNode {
    EmpowerJaceEffectNode {
        player: LocalPlayerReference::Controller,
        count,
    }
}

#[test]
fn keyword_action_expands_to_conditional_creation_choice_and_loyalty_counters() {
    let effects = lower_empower_jace_effects(
        &empower(NumericExpression::from(2)),
        TargetlessDeclarationSupport::None,
        Some("effect1"),
    )
    .unwrap();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["condition"]["kind"], "not");
    assert_eq!(effects[0]["then"][0]["kind"], "createTokens");
    assert_eq!(effects[0]["then"][0]["token"]["kind"], "namedToken");
    assert_eq!(effects[0]["then"][0]["token"]["name"], "Jace");
    assert_eq!(effects[0]["then"][0]["token"]["colors"][0], "Blue");
    let chosen = &effects[1]["then"];
    assert_eq!(chosen[0]["kind"], "choosePermanents");
    assert_eq!(chosen[0]["id"], "effect1:empoweredJace");
    assert_eq!(chosen[1]["kind"], "putCounters");
    assert_eq!(chosen[1]["counter"], "loyalty");
    assert_eq!(chosen[1]["count"]["value"], 2);
}

#[test]
fn distinct_scopes_get_distinct_choice_ids_and_zero_is_representable() {
    let first = lower_empower_jace_effects(
        &empower(NumericExpression::from(0)),
        TargetlessDeclarationSupport::None,
        Some("effect0"),
    )
    .unwrap();
    let second = lower_empower_jace_effects(
        &empower(NumericExpression::from(0)),
        TargetlessDeclarationSupport::None,
        Some("effect1"),
    )
    .unwrap();
    assert_ne!(first[1]["then"][0]["id"], second[1]["then"][0]["id"]);
}

#[test]
fn an_undeclared_x_count_is_not_executable() {
    assert!(
        lower_empower_jace_effects(
            &empower(NumericExpression::Atom(NumericAtom::VariableX)),
            TargetlessDeclarationSupport::None,
            None,
        )
        .is_err()
    );
}

/// The engine runtime test (`empower_jace_runtime`) executes this exact
/// program; keep both in step.
#[test]
fn lowered_program_matches_the_one_executed_by_the_engine_proof() {
    let you = json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    let jace = json!({
        "kind": "and",
        "operands": [
            { "kind": "isToken" },
            { "kind": "cardTypeContains", "value": "Planeswalker" },
            { "kind": "subtypeContains", "value": "Jace" },
        ],
    });
    let controls = json!({ "kind": "controlsPermanent", "where": jace });
    let expected = json!([
        {
            "kind": "conditional",
            "condition": { "kind": "not", "operand": controls },
            "then": [{
                "kind": "createTokens",
                "controller": you,
                "quantity": { "kind": "integer", "value": 1 },
                "token": { "kind": "namedToken", "name": "Jace", "colors": ["Blue"] },
            }],
        },
        {
            "kind": "conditional",
            "condition": controls,
            "then": [
                {
                    "kind": "choosePermanents",
                    "id": "effect0:empoweredJace",
                    "player": you,
                    "candidates": { "kind": "permanents", "controller": you, "where": jace },
                    "minimum": { "kind": "integer", "value": 1 },
                    "maximum": { "kind": "integer", "value": 1 },
                },
                {
                    "kind": "putCounters",
                    "permanent": { "kind": "decisionResult", "decisionId": "effect0:empoweredJace" },
                    "counter": "loyalty",
                    "count": { "kind": "integer", "value": 2 },
                },
            ],
        },
    ]);
    let lowered = lower_empower_jace_effects(
        &empower(NumericExpression::from(2)),
        TargetlessDeclarationSupport::None,
        Some("effect0"),
    )
    .unwrap();
    assert_eq!(json!(lowered), expected);
}
