//! `you may ACTION. If you do / If you don't / When you do, BODY.`: an
//! optional instruction (not a cost) whose outcome follows actual performance.
use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};

fn parse(text: &str, face_type_line: &str, kind: &str) -> Result<Value, MachineFailure> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Optional Action Source",
        face_type_line,
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
    parse(text, "Creature — Wall", "triggeredAbility")
        .unwrap_or_else(|failure| panic!("{text}: {} expected {}", failure.state, failure.expected))
}

fn spell(text: &str) -> Value {
    parse(text, "Sorcery", "spellAbility")
        .unwrap_or_else(|failure| panic!("{text}: {} expected {}", failure.state, failure.expected))
}

fn controller() -> Value {
    json!({"kind":"controllerOf","object":{"kind":"self"}})
}

fn kinds(effects: &Value) -> Vec<&str> {
    effects
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap_or_default())
        .collect()
}

#[test]
fn academy_wall_draws_then_discards_only_if_it_drew() {
    let rule = trigger(
        "Whenever you cast an instant or sorcery spell, you may draw a card. If you do, \
         discard a card. This ability triggers only once each turn.",
    );
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "optionalAction",
            "player": controller(),
            "action": {"kind":"drawCards","player":controller(),"count":{"kind":"integer","value":1}},
            "onPerformed": [{
                "kind":"discardCards","player":controller(),"count":{"kind":"integer","value":1}
            }],
        }])
    );
    // The trigger parent still owns its limit sentence.
    assert_eq!(rule["triggerLimit"]["kind"], "onceEachTurn");
}

#[test]
fn if_you_dont_runs_only_when_the_instruction_was_not_performed() {
    // Devouring Sugarmaw: the coordinated sacrifice operand is one closed
    // object list (roadmap #12), so it is the same optional cost a single
    // operand is; declining it runs the `If you don't` effects.
    let rule = trigger(
        "At the beginning of your upkeep, you may sacrifice an artifact, enchantment, or \
         token. If you don't, tap this creature.",
    );
    let block = &rule["effects"][0];
    assert_eq!(block["kind"], "optionalPayCostPerformEffects");
    assert_eq!(block["cost"]["kind"], "sacrificePermanent");
    assert_eq!(block["cost"]["where"]["kind"], "or");
    assert_eq!(block["effects"], json!([]));
    assert_eq!(kinds(&block["declineEffects"]), ["tapPermanent"]);
}

#[test]
fn when_you_do_creates_a_reflexive_trigger_with_its_own_targets() {
    // The Fire Nation Drill (CR 603.12).
    let rule = trigger(
        "When this creature enters, you may tap it. When you do, destroy target creature \
         with power 4 or less.",
    );
    assert!(rule.get("declaration").is_none());
    let block = &rule["effects"][0];
    assert_eq!(
        block["action"],
        json!({"kind":"tapPermanent","permanent":{"kind":"self"}})
    );
    let reflexive = &block["onPerformed"][0];
    assert_eq!(reflexive["kind"], "createReflexiveTrigger");
    assert_eq!(
        reflexive["ability"]["declaration"]["decisions"][0]["id"],
        "targetPermanent"
    );
    assert_eq!(
        kinds(&reflexive["ability"]["effects"]),
        ["destroyPermanent"]
    );
}

#[test]
fn the_instruction_target_is_declared_by_the_enclosing_ability() {
    // Carrion Imp.
    let rule = trigger(
        "When this creature enters, you may exile target creature card from a graveyard. \
         If you do, you gain 2 life.",
    );
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetCard");
    let block = &rule["effects"][0];
    assert_eq!(block["action"]["kind"], "exileTargetCards");
    assert_eq!(kinds(&block["onPerformed"]), ["gainLife"]);
}

#[test]
fn optional_instruction_is_an_independent_step_between_ordinary_sentences() {
    // New combination: leading step, `If you don't` block, trailing step.
    let rule = spell(
        "Draw a card. You may exile target creature. If you don't, you lose 2 life. \
         You gain 1 life.",
    );
    assert_eq!(
        kinds(&rule["effects"]),
        ["drawCards", "optionalAction", "gainLife"]
    );
    let block = &rule["effects"][1];
    assert_eq!(block["action"]["kind"], "exilePermanent");
    assert_eq!(kinds(&block["onDeclined"]), ["loseLife"]);
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
}

#[test]
fn a_typed_resolution_cost_keeps_the_cost_program() {
    let rule = spell("You may discard a card. If you do, draw two cards.");
    assert_eq!(kinds(&rule["effects"]), ["optionalPayCostPerformEffects"]);
}

#[test]
fn unmeasured_or_compound_instructions_fail_explicitly() {
    for text in [
        // Scry is a look/choose/move program, not one measured instruction.
        "When this creature enters, you may scry 2. If you do, draw a card.",
        // Two instructions: which one must be performed is not one primitive.
        "When this creature enters, you may draw a card and gain 1 life. If you do, \
         discard a card.",
    ] {
        let failure = parse(text, "Creature — Wall", "triggeredAbility").unwrap_err();
        assert_eq!(
            failure.state, "lower.program.optional-action.action",
            "{text}"
        );
    }
}

#[test]
fn colliding_declarations_and_second_outcomes_fail_explicitly() {
    let failure = parse(
        "When this creature enters, you may exile target creature. If you do, destroy \
         target artifact.",
        "Creature — Wall",
        "triggeredAbility",
    )
    .unwrap_err();
    assert_eq!(failure.state, "lower.program.optional-action.declarations");
    // An outcome governs its own sentence only; a second outcome is not a step.
    assert!(
        parse(
            "When this creature enters, you may draw a card. If you do, discard a card. \
             If you don't, you lose 2 life.",
            "Creature — Wall",
            "triggeredAbility",
        )
        .is_err()
    );
}
