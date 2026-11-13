use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities, resolve_counter_name_spans},
};

use super::*;

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_oracle_entities(text);
    resolve_counter_name_spans(&mut tokens);
    parse_promoted_labeled_entry_reflexive_counter(&AbilityInput {
        face_name: "Generalized Test Enchantment",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn claims(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_oracle_entities(text);
    resolve_counter_name_spans(&mut tokens);
    claims_promoted_labeled_entry_reflexive_counter_family(&AbilityInput {
        face_name: "Generalized Test Enchantment",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn lowers_exact_shape_to_counter_reflexive_condition_target_and_keyword_primitives() {
    let draft = parse(
        "Landfall \u{2014} Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.",
    )
    .expect("the exact ability should lower through native typed composition");
    let rule = draft.rule;

    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(rule["event"]["kind"], "permanentEntered");
    assert_eq!(rule["event"]["where"]["kind"], "cardTypeContains");
    assert_eq!(rule["event"]["where"]["value"], "Land");
    assert_eq!(rule["effects"][0]["kind"], "putCounters");
    assert_eq!(rule["effects"][0]["counter"], "quest");
    assert_eq!(rule["effects"][0]["bind"], PLACED_COUNTERS_BINDING);
    assert_eq!(rule["effects"][1]["kind"], "conditionalEffect");

    let reflexive = &rule["effects"][1]["then"][0]["ability"];
    assert_eq!(reflexive["kind"], "triggeredAbility");
    assert_eq!(reflexive["event"]["kind"], "reflexiveTriggerCreated");
    assert_eq!(reflexive["condition"]["operator"], ">=");
    assert_eq!(reflexive["condition"]["left"]["kind"], "countCounters");
    assert_eq!(reflexive["condition"]["left"]["counter"], "quest");
    assert_eq!(
        reflexive["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "controllerOf"
    );
    assert_eq!(reflexive["effects"][0]["kind"], "putCounters");
    assert_eq!(reflexive["effects"][0]["counter"], "+1/+1");
    assert_eq!(reflexive["effects"][1]["kind"], "grantKeyword");
    assert_eq!(reflexive["effects"][1]["keyword"], "trample");
    assert_eq!(
        reflexive["effects"][1]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn sibling_lowers_different_opaque_counters_threshold_target_and_keyword() {
    let rule = parse(
        "Will of the council \u{2014} Whenever an artifact you control enters, put two charge counters on this artifact. When you do, if it has three or fewer charge counters on it, put a shield counter on target artifact creature you control. It gains flying until end of turn.",
    )
    .expect("the sibling should share the same typed grammar")
    .rule;

    assert_eq!(rule["event"]["where"]["value"], "Artifact");
    assert_eq!(rule["effects"][0]["counter"], "charge");
    assert_eq!(rule["effects"][0]["count"]["value"], 2);
    let reflexive = &rule["effects"][1]["then"][0]["ability"];
    assert_eq!(reflexive["condition"]["operator"], "<=");
    assert_eq!(reflexive["condition"]["right"]["value"], 3);
    assert_eq!(reflexive["effects"][0]["counter"], "shield");
    assert_eq!(reflexive["effects"][1]["keyword"], "flying");
}

#[test]
fn malformed_reflexive_actor_is_claimed_then_fails_without_legacy_fallback() {
    let text = "Landfall \u{2014} Whenever a land you control enters, put a quest counter on this enchantment. When an opponent does, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.";
    assert!(claims(text));
    let failure = match parse(text) {
        Ok(_) => panic!("the committed family must expose its local failure"),
        Err(failure) => failure,
    };
    assert_eq!(
        failure.state,
        "trigger.labeled-reflexive-counter.reflexive-player"
    );
}

#[test]
fn unrelated_labeled_entry_without_reflexive_counter_shape_is_not_claimed() {
    assert!(!claims(
        "Landfall \u{2014} Whenever a land you control enters, draw a card."
    ));
}
