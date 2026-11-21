use super::*;

fn failure(state: &'static str) -> MachineFailure {
    let tokens = crate::oracle::syntax::recognize_entities("draw");
    let mut cursor = OracleTokenCursor::new("dispatch-ownership-test", &tokens);
    cursor.enter(state);
    cursor.fail("the owning grammar contract")
}

#[test]
fn complete_compatibility_rule_wins_without_modifying_its_contract() {
    // This opaque sentinel tests dispatch transport, not executable coverage.
    let rule = json!({"kind":"old-complete-contract","caseSensitive":"RetainThisID"});
    let result = preserve_committed_generic_failure(
        CanonicalParseOutcome::Parsed {
            draft: draft(rule.clone(), &[]),
            recognizer: RecognizerProvenance::Native,
        },
        Some(failure("owned-new-failure")),
    );
    let CanonicalParseOutcome::Parsed { draft, .. } = result else {
        panic!("a complete prior contract must retain precedence")
    };
    assert_eq!(draft.rule, rule);
}

#[test]
fn owning_failure_survives_unsupported_compatibility_without_a_candidate() {
    let committed = failure("owned-new-failure");
    let expected = committed.clone();
    let CanonicalParseOutcome::Failed { failure, .. } =
        preserve_committed_generic_failure(CanonicalParseOutcome::Unsupported, Some(committed))
    else {
        panic!("unsupported compatibility supplies no additional failure")
    };
    assert_eq!(failure, expected);
}

#[test]
fn compatibility_failure_is_retained_as_candidate_without_replacing_the_owner() {
    let committed = failure("owned-new-failure");
    let candidate = failure("candidate-old-failure");
    let expected_primary = committed.clone();
    let expected_candidate = candidate.clone();
    let CanonicalParseOutcome::FailedWithCandidateEvidence {
        failure,
        candidate_failure,
        ..
    } = preserve_committed_generic_failure(
        CanonicalParseOutcome::Failed {
            failure: candidate,
            recognizer: RecognizerProvenance::Native,
        },
        Some(committed),
    )
    else {
        panic!("both roles must survive explicitly")
    };
    assert_eq!(
        failure, expected_primary,
        "preserve owned message, spans and full transition trace"
    );
    assert_eq!(
        candidate_failure, expected_candidate,
        "retain candidate fields without inferring commitment"
    );
}

#[test]
fn no_commit_preserves_the_original_compatibility_diagnostic() {
    let original = failure("original-compatibility-state");
    let expected = original.clone();
    let CanonicalParseOutcome::Failed { failure, .. } = preserve_committed_generic_failure(
        CanonicalParseOutcome::Failed {
            failure: original,
            recognizer: RecognizerProvenance::Native,
        },
        None,
    ) else {
        panic!("no owner means existing failure is retained")
    };
    assert_eq!(failure, expected);
}
