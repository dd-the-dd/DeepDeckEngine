use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

fn with_input<T>(text: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    operation(&AbilityInput {
        face_name: "Generalized Death Source",
        face_type_line: "Creature — Cleric",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    with_input(text, parse_promoted_permanent_died_effect_sequence)
}

fn claims(text: &str) -> bool {
    with_input(text, claims_promoted_permanent_died_effect_sequence_family)
}

#[test]
fn lowers_source_or_other_creature_and_targeted_life_as_independent_primitives() {
    let text = "Whenever this creature or another creature dies, target player loses 1 life and you gain 1 life.";
    assert!(claims(text));
    let rule = parse(text)
        .expect("death and life sequence should lower")
        .rule;
    assert_eq!(rule["event"]["kind"], "oneOf");
    assert_eq!(rule["event"]["events"][0]["object"]["kind"], "self");
    assert_eq!(rule["event"]["events"][1]["excludeSource"], true);
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPlayer");
    assert_eq!(rule["effects"][0]["kind"], "loseLife");
    assert_eq!(rule["effects"][1]["kind"], "gainLife");
}

#[test]
fn reuses_one_player_target_for_that_player_sibling() {
    let rule = parse(
        "Whenever another creature dies, target player gains 2 life and that player loses 1 life.",
    )
    .expect("player anaphor should reuse the first target")
    .rule;
    assert_eq!(
        rule["declaration"]["decisions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(rule["effects"][0]["player"]["id"], "targetPlayer");
    assert_eq!(rule["effects"][1]["player"]["id"], "targetPlayer");
}

#[test]
fn claimed_missing_separator_reports_the_parent_state() {
    let text = "Whenever another creature dies target player loses 1 life.";
    assert!(claims(text));
    let failure = parse(text).expect_err("claimed malformed family must remain native failure");
    assert_eq!(failure.state, "trigger.permanent-died.separator");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}
