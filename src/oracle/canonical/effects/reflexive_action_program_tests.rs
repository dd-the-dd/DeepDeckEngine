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
fn if_you_do_after_a_mandatory_instruction_gates_the_body_on_its_performance() {
    // CR 608.2c: the connector observes the instruction's ActionResult. It is
    // not reflexive: the body runs inside the resolving ability and its
    // targets belong to the enclosing declaration.
    // Unused combination: a discard instruction gating a draw.
    let rule = trigger("When this artifact enters, discard a card. If you do, draw two cards.");
    let block = &rule["effects"][0];
    assert_eq!(block["kind"], "performAction");
    assert_eq!(block["action"]["kind"], "discardCards");
    assert_eq!(block["onPerformed"][0]["kind"], "drawCards");
    assert!(block["onPerformed"][0].get("ability").is_none());
}

#[test]
fn a_declined_outcome_after_a_mandatory_instruction_fails_explicitly() {
    // Near miss: a mandatory instruction has no declined branch to observe.
    assert!(
        parse(
            "When this artifact enters, draw a card. If you don't, you gain 2 life.",
            "triggeredAbility",
        )
        .is_err()
    );
}

#[test]
fn a_connector_subject_must_name_the_permission_performer() {
    // The ActionResult antecedent of `you may` is the controller: `If they
    // do` names nobody here and fails instead of guessing a performer.
    assert!(
        parse(
            "When this artifact enters, you may draw a card. If they do, you gain 2 life.",
            "triggeredAbility",
        )
        .is_err()
    );
}
