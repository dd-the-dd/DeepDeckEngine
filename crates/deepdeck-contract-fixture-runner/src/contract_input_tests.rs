use super::*;
use serde_json::json;

fn input(text: &str) -> Value {
    json!({
        "id":"synthetic-input-contract",
        "oracle": {
            "cardName":"Synthetic Input Contract", "typeLine":"Instant",
            "manaCost":"{U}", "oracleText":text,
            "layout":"normal", "faces":[],
        }
    })
}

fn fixture(extra: Value) -> ContractFixture {
    let mut value = json!({"schemaVersion":FIXTURE_SCHEMA_VERSION,"id":"synthetic/contract-input","expectedExecutable":true});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(value).expect("contract fixture schema deserializes")
}

#[test]
fn old_ir_fixture_shapes_and_nonempty_requirement_stay_valid() {
    assert!(
        serde_json::from_value::<ContractFixture>(json!({"schemaVersion":FIXTURE_SCHEMA_VERSION,
        "id":"legacy/null-rules", "expectedExecutable":false, "rules":null}))
        .is_err()
    );
    for extra in [
        json!({"rule":{"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"flying"}}}),
        json!({"rules":[{"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"flying"}}]}),
    ] {
        let fixture = fixture(extra);
        validate_fixture_metadata(&fixture).unwrap();
        let result = evaluate_fixture(&fixture);
        assert!(result.executable);
        assert!(result.parser_status.is_none());
        assert!(result.engine_status.is_none());
    }
    assert!(validate_fixture_metadata(&fixture(json!({"rules":[]}))).is_err());
}

#[test]
fn compiled_input_and_rule_specifications_are_mutually_exclusive_even_for_empty_rules() {
    for extra in [
        json!({"input":input("Draw a card."),"rule":{"kind":"unknown"}}),
        json!({"input":input("Draw a card."),"rules":[]}),
        json!({"input":input("Draw a card."),"rules":[{"kind":"unknown"}]}),
    ] {
        let failure = validate_fixture_metadata(&fixture(extra)).unwrap_err();
        assert!(failure.contains("conflicts"));
    }
    assert!(
        validate_fixture_metadata(&fixture(
            json!({"rule":{"kind":"unknown"},"expectedEngineStatus":"incomplete"})
        ))
        .is_err()
    );
}

#[test]
fn unsupported_source_with_empty_filtered_rules_is_incomplete_not_executable() {
    let fixture = fixture(
        json!({"input":input("Sing a song."),"expectedExecutable":false,
        "expectedParserStatus":"unsupported","expectedEngineStatus":"incomplete"}),
    );
    validate_fixture_metadata(&fixture).unwrap();
    let compiled = compile_playable_card(fixture.input.clone().unwrap()).unwrap();
    assert!(
        compiled.card.rules.is_empty(),
        "negative must exercise the historical vacuous-all risk"
    );
    assert!(compiled.unsupported_ability_count > 0);
    let evaluation = evaluate_fixture(&fixture);
    assert!(!evaluation.executable);
    assert!(evaluation.failure(&fixture).is_none());
}

#[test]
fn input_status_expectations_are_assertions_and_structure_failures_never_pass_as_false() {
    let mut fixture = fixture(
        json!({"input":input("Draw a card."),"expectedParserStatus":"canonical","expectedEngineStatus":"executable"}),
    );
    let evaluation = evaluate_fixture(&fixture);
    assert!(evaluation.executable);
    assert!(evaluation.failure(&fixture).is_none());
    fixture.expected_engine_status = Some("incomplete".to_string());
    assert!(
        evaluation
            .failure(&fixture)
            .unwrap()
            .contains("engine status mismatch")
    );
    fixture.input.as_mut().unwrap().oracle.layout = Some("future_unknown_layout".to_string());
    fixture.expected_executable = false;
    let evaluation = evaluate_fixture(&fixture);
    assert!(!evaluation.executable);
    assert!(
        evaluation
            .failure(&fixture)
            .unwrap()
            .contains("card_face_factory_missing")
    );
}
