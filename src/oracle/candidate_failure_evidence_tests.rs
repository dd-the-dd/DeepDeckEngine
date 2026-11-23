use super::*;

#[test]
fn generic_primary_and_specialized_candidate_are_both_exposed_with_distinct_roles() {
    let fixtures: &[(&str, &str, &str, &str, &str)] = &[
        (
            include_str!(
                "../../crates/deepdeck-parser-fixture-runner/fixtures/earthbender-ascension-landfall.json"
            ),
            "unsupported-reflexive-actor-near-miss",
            "effect.start",
            "trigger.labeled-reflexive-counter.reflexive-player",
            "an",
        ),
        (
            include_str!(
                "../../crates/deepdeck-parser-fixture-runner/fixtures/earthbender-ascension-landfall.json"
            ),
            "unsupported-source-counter-comparator-near-miss",
            "effect.start",
            "condition.source-counter-threshold.comparator-direction",
            "exactly",
        ),
        (
            include_str!(
                "../../crates/deepdeck-parser-fixture-runner/fixtures/phelia-exuberant-shepherd.json"
            ),
            "unsupported-delayed-step-near-miss",
            "delayed-step.timing.step",
            "duration.next-step.step",
            "combat",
        ),
        (
            include_str!(
                "../../crates/deepdeck-parser-fixture-runner/fixtures/spell-tax-claim-boundary.json"
            ),
            "synthetic-tax-wrong-payer",
            "sequence.connector-or-end",
            "reference.triggering-player.determiner",
            "an",
        ),
    ];
    for (raw, id, primary_state, candidate_state, candidate_found) in fixtures {
        let fixture: Value = serde_json::from_str(raw).unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == *id)
            .unwrap();
        let request: OracleCardParseRequest =
            serde_json::from_value(case["request"].clone()).unwrap();
        let result = parse_oracle_card_native(request);
        assert_eq!(result.status, "unsupported", "{id}");
        let ability = &result.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{id}");
        assert!(
            ability.rule.is_none(),
            "candidate evidence is not an executable parse"
        );
        assert_eq!(
            ability.diagnostics[0].code, "oracle_machine_failure",
            "{id}: primary remains first"
        );
        let primary = ability
            .diagnostics
            .iter()
            .find(|d| d.code == "oracle_machine_failure")
            .unwrap();
        let candidate = ability
            .diagnostics
            .iter()
            .find(|d| d.code == "oracle_candidate_machine_failure")
            .unwrap();
        assert_eq!(
            primary.automaton.as_deref(),
            Some("generic-trigger-program"),
            "{id}"
        );
        assert_eq!(primary.state.as_deref(), Some(*primary_state), "{id}");
        assert_eq!(candidate.state.as_deref(), Some(*candidate_state), "{id}");
        assert_eq!(candidate.found.as_deref(), Some(*candidate_found), "{id}");
        assert!(
            candidate.span_start.is_some()
                && candidate.span_end.is_some()
                && candidate.token_index.is_some(),
            "candidate retains source location"
        );
        assert!(
            !candidate.trace.is_empty(),
            "candidate retains its own transition trace"
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "oracle_candidate_machine_failure"),
            "top-level feedback also retains the candidate"
        );
    }
}
