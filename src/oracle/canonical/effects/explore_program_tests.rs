use super::*;

fn kinds(value: &Value, result: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                result.push(kind.to_string());
            }
            for value in object.values() {
                kinds(value, result);
            }
        }
        Value::Array(values) => {
            for value in values {
                kinds(value, result);
            }
        }
        _ => {}
    }
}

#[test]
fn empty_library_still_reaches_counter_branch_and_one_completion() {
    let effects =
        lower_explore_program(json!({"kind":"self"}), "example").expect("registered program");
    // The source reference is used directly (its last known information is
    // engine-captured); only its controller is bound before the procedure.
    assert_eq!(effects[0]["id"], "example:controller");
    assert_eq!(
        effects[3]["condition"]["left"]["objects"]["where"]["value"],
        "Land"
    );
    assert_eq!(
        effects[3]["else"][0]["condition"]["kind"],
        "objectOnBattlefield"
    );
    assert_eq!(effects[3]["else"][0]["then"][0]["kind"], "putCounters");
    assert_eq!(
        effects[3]["else"][1]["condition"]["left"]["objects"]["binding"],
        "example:top"
    );
    let mut found = Vec::new();
    kinds(&json!(effects), &mut found);
    assert_eq!(
        found
            .iter()
            .filter(|kind| kind.as_str() == "emitAbilitySignal")
            .count(),
        1
    );
    assert!(!found.iter().any(|kind| kind == "explore"));
}

#[test]
fn scopes_are_independent_and_the_signal_names_the_exploring_object() {
    let first = lower_explore_program(json!({"kind":"self"}), "first").unwrap();
    let second =
        lower_explore_program(json!({"kind":"chosenTarget","id":"recipient"}), "second").unwrap();
    assert_eq!(first[0]["id"], "first:controller");
    assert_eq!(first.last().unwrap()["object"]["kind"], "self");
    assert_eq!(second[0]["id"], "second:subject");
    assert_eq!(second.last().unwrap()["object"]["id"], "second:subject");
    assert_eq!(second.last().unwrap()["abilityId"], "explore");
    assert!(lower_explore_program(json!({"kind":"self"}), "").is_err());
}

#[test]
fn actual_map_compiler_keeps_costs_target_and_timing_but_emits_no_explore_opcode() {
    let source: Value =
        serde_json::from_str(include_str!("../../../../data/builtin-game-pieces.json")).unwrap();
    let oracle = source["cards"]["map"][0]["oracleText"].as_str().unwrap();
    let rule = crate::oracle::compile_named_token_native_rule(oracle).expect("actual Map grammar");
    assert_eq!(rule["activationCondition"]["kind"], "sorceryTiming");
    assert_eq!(rule["costs"].as_array().unwrap().len(), 3);
    assert_eq!(rule["costs"][2]["kind"], "sacrificePermanent");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Creature"
    );
    assert_eq!(rule["effects"].as_array().unwrap().len(), 6);
    let mut found = Vec::new();
    kinds(&rule, &mut found);
    assert!(!found.iter().any(|kind| kind == "explore"));
}

#[test]
fn unsupported_token_instructions_do_not_emit_a_partial_exploration_program() {
    for oracle in [
        "{1}, {T}, Sacrifice this token: Target creature you control explores and gains flying.",
        "{1}, {T}, Sacrifice this token: Target creature you control explores. Gain 1 life.",
    ] {
        assert!(
            crate::oracle::compile_named_token_native_rule(oracle).is_none(),
            "unsupported complete envelope must not produce a partial program: {oracle}"
        );
    }
}

#[test]
fn another_permanent_type_reuses_the_complete_exploration_program() {
    let creature = crate::oracle::compile_named_token_native_rule(
        "{1}, {T}, Sacrifice this token: Target creature you control explores.",
    )
    .expect("the shared token instruction grammar accepts this permanent selector");
    let artifact = crate::oracle::compile_named_token_native_rule(
        "{1}, {T}, Sacrifice this token: Target artifact you control explores.",
    )
    .expect("exploration is not restricted to creature permanents");
    let mut expected = creature;
    expected["declaration"]["decisions"][0]["candidates"]["where"]["value"] = json!("Artifact");
    assert_eq!(artifact, expected);
}
