//! `ACTION. When you do, BODY.` after a mandatory instruction (CR 603.12):
//! the instruction's measured performance creates the reflexive ability.
use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};

fn parse(text: &str, kind: &str) -> Result<Value, MachineFailure> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Reflexive Source",
        face_type_line: "Artifact",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule_native(&input, Some(kind)) {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => Err(failure),
        CanonicalParseOutcome::Unsupported => panic!("{text}: unclaimed"),
    }
}

fn trigger(text: &str) -> Value {
    parse(text, "triggeredAbility")
        .unwrap_or_else(|failure| panic!("{text}: {} expected {}", failure.state, failure.expected))
}

#[test]
fn smoke_bomb_sacrifice_creates_a_targeted_reflexive_ability() {
    let rule = trigger(
        "At the beginning of your upkeep, sacrifice this artifact. When you do, target \
         creature you control can't be blocked this turn.",
    );
    // The reflexive ability declares its own target; the trigger declares none.
    assert!(rule.get("declaration").is_none());
    let block = &rule["effects"][0];
    assert_eq!(block["kind"], "performAction");
    assert_eq!(block["action"]["kind"], "sacrificePermanent");
    let reflexive = &block["onPerformed"][0];
    assert_eq!(reflexive["kind"], "createReflexiveTrigger");
    assert!(reflexive["ability"]["declaration"]["decisions"][0]["id"].is_string());
}

#[test]
fn a_mandatory_draw_reuses_the_measured_instruction_contract() {
    // Unused combination: draw instruction, life outcome, no targets.
    let rule = trigger("When this artifact enters, draw a card. When you do, you gain 2 life.");
    let block = &rule["effects"][0];
    assert_eq!(block["kind"], "performAction");
    assert_eq!(block["action"]["kind"], "drawCards");
    assert_eq!(
        block["onPerformed"][0]["ability"]["effects"][0]["kind"],
        "gainLife"
    );
    assert!(
        block["onPerformed"][0]["ability"]
            .get("declaration")
            .is_none()
    );
}

#[test]
fn if_you_do_after_a_mandatory_instruction_is_not_a_reflexive_outcome() {
    // Near miss: `If you do` has no permission to observe and stays unsupported.
    assert!(
        parse(
            "When this artifact enters, draw a card. If you do, you gain 2 life.",
            "triggeredAbility",
        )
        .is_err()
    );
}
