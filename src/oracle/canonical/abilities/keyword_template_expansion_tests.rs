use serde_json::json;

use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::recognize_oracle_entities;

const LINE: &str = "Battle cry";

fn expand(compile: &mut dyn FnMut(&str) -> CanonicalParseOutcome) -> CanonicalParseOutcome {
    let source = AbilitySource {
        face_id: None,
        text: LINE.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(LINE);
    let input = AbilityInput {
        face_name: "Probe",
        face_type_line: "Creature \u{2014} Human Knight",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    expand_keyword_template_line(&input, compile).expect("a template keyword line")
}

fn inner_failure() -> MachineFailure {
    MachineFailure {
        automaton: "generic-trigger-program",
        state: "event.source.verb",
        token_index: 3,
        span_start: 10,
        span_end: 14,
        expected: "a supported event verb".to_string(),
        actual: Some("attacks".to_string()),
        trace: Vec::new(),
        owner: None,
    }
}

#[test]
fn a_part_the_shared_parser_rejects_fails_the_keyword_with_the_part_as_evidence() {
    let mut seen = Vec::new();
    let outcome = expand(&mut |text: &str| {
        seen.push(text.to_string());
        CanonicalParseOutcome::Failed {
            failure: inner_failure(),
            recognizer: RecognizerProvenance::Native,
        }
    });
    assert_eq!(
        seen,
        [
            "Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn."
        ]
    );
    let CanonicalParseOutcome::FailedWithCandidateEvidence {
        failure,
        candidate_failure,
        ..
    } = outcome
    else {
        panic!("an explicit expansion failure");
    };
    assert_eq!(failure.automaton, "keyword-template");
    assert_eq!(failure.state, "keyword.template.expansion");
    assert!(
        failure.expected.contains("battle-cry"),
        "{}",
        failure.expected
    );
    assert!(
        failure.expected.contains("event.source.verb"),
        "{}",
        failure.expected
    );
    assert_eq!((failure.span_start, failure.span_end), (0, LINE.len()));
    assert_eq!(candidate_failure, inner_failure());
}

#[test]
fn an_unclaimed_part_is_an_explicit_failure_too() {
    let outcome = expand(&mut |_: &str| CanonicalParseOutcome::Unsupported);
    let CanonicalParseOutcome::Failed { failure, .. } = outcome else {
        panic!("an explicit expansion failure");
    };
    assert_eq!(failure.state, "keyword.template.expansion");
    assert!(
        failure.expected.contains("no ability reader"),
        "{}",
        failure.expected
    );
}

#[test]
fn flattening_carries_placement_and_provenance_to_every_expanded_rule() {
    let expansion = json!({
        "kind": "keywordExpansion",
        "source": { "kind": "self" },
        "keyword": "probe",
        "parameters": { "N": 2 },
        "definition": { "rulesSection": "702.1a", "templateVersion": 3 },
        "rules": [{ "kind": "triggeredAbility" }, { "kind": "replacementEffect" }],
        "minimumClassLevel": 2,
    });
    let rules = keyword_expansion_rules(&expansion);
    assert_eq!(rules.len(), 2);
    for (part, rule) in rules.iter().enumerate() {
        assert_eq!(rule["minimumClassLevel"], 2);
        assert_eq!(
            rule["keywordProvenance"],
            json!({
                "keyword": "probe",
                "parameters": { "N": 2 },
                "rulesSection": "702.1a",
                "templateVersion": 3,
                "part": part,
            })
        );
        assert!(rule.get("rules").is_none() && rule.get("definition").is_none());
    }
    // An ordinary rule and a malformed expansion are returned unchanged.
    let ordinary = json!({ "kind": "triggeredAbility" });
    assert_eq!(keyword_expansion_rules(&ordinary), vec![ordinary.clone()]);
    let malformed = json!({ "kind": "keywordExpansion" });
    assert_eq!(keyword_expansion_rules(&malformed), vec![malformed.clone()]);
}
