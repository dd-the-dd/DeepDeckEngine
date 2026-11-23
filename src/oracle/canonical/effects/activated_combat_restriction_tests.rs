use super::super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

/// Activated combat restrictions are owned by the shared activation-cost and
/// effect-sequence parents; this runs the public native dispatcher.
fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Combat Restriction Source",
        face_type_line: "Land",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule(&input, "activatedAbility") {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => Err(failure),
        CanonicalParseOutcome::Unsupported => panic!("no native owner for {text}"),
    }
}

#[test]
fn lowers_cost_target_restriction_and_duration_as_primitives() {
    let parsed = parse("{4}, {T}: Target creature can't be blocked this turn.")
        .expect("a complete activated combat restriction must parse");
    assert_eq!(parsed.rule["costs"][0]["manaCost"], "{4}");
    assert_eq!(parsed.rule["costs"][1]["kind"], "tap");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "targetPermanent"
    );
    assert_eq!(parsed.rule["effects"][0]["keyword"], "cantBeBlocked");
    assert_eq!(
        parsed.rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
}

#[test]
fn malformed_combat_duration_remains_a_hard_failure() {
    let failure = parse("{2}, {T}: Target creature can't be blocked this combat.")
        .expect_err("a committed malformed duration must remain a hard failure");
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");
}

#[test]
fn selected_sacrifice_cost_composes_with_the_restriction() {
    let supported = parse("{R}, Sacrifice an artifact: Target creature can't block this turn.")
        .expect("existing selected sacrifice cost must remain supported");
    assert_eq!(supported.rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(
        supported.rule["declaration"]["decisions"][0]["kind"],
        "chooseObjects"
    );
    assert_eq!(
        supported.rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Artifact"
    );
    assert_eq!(supported.rule["effects"][0]["keyword"], "cantBlock");
}

#[test]
fn target_collection_restriction_uses_the_shared_plural_reference() {
    let parsed = parse("{4}, {T}: Two target creatures can't be blocked this turn.")
        .expect("a permanent target collection composes with the restriction");
    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 2);
    assert_eq!(decision["maximum"], 2);
    assert_eq!(parsed.rule["effects"][0]["object"]["kind"], "chosenTargets");
    assert_eq!(parsed.rule["effects"][0]["object"]["id"], decision["id"]);
}

#[test]
fn unsupported_selector_threshold_is_not_approximated() {
    let failure = parse("{T}: Target creature with power enormous can't be blocked this turn.")
        .expect_err("an unsupported numeric threshold must not parse");
    // The dispatcher owns the numeric child's sink: the frame that asked for
    // the threshold, then the class of the word it met (`enormous`).
    assert!(failure.state.ends_with("/numeric.word"), "{failure}");
    assert!(
        failure
            .trace
            .iter()
            .any(|transition| transition.state == "numeric.atom.start"),
        "{failure}"
    );
}
