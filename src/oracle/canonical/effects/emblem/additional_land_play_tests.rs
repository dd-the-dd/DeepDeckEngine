use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    parse_promoted_additional_land_play(&AbilityInput {
        face_name: "Generic Explorer",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn one_land_lowers_to_the_existing_static_modifier() {
    let draft = parse("You may play an additional land on each of your turns.")
        .expect("one-land allowance lowers");

    assert_eq!(draft.rule["kind"], "staticAbility");
    assert_eq!(draft.rule["activeWhile"]["zone"]["kind"], "battlefield");
    assert_eq!(draft.rule["modifiers"][0]["kind"], "additionalLandPlay");
    assert_eq!(draft.rule["modifiers"][0]["player"]["kind"], "controllerOf");
    assert_eq!(draft.rule["modifiers"][0]["count"]["kind"], "integer");
    assert_eq!(draft.rule["modifiers"][0]["count"]["value"], 1);
}

#[test]
fn two_lands_preserve_the_numeric_count_field() {
    let draft = parse("You may play two additional lands on each of your turns.")
        .expect("two-land allowance lowers");

    assert_eq!(draft.rule["modifiers"][0]["count"]["value"], 2);
    assert!(crate::engine::rule_is_executable(&draft.rule));
}

#[test]
fn variable_allowance_is_not_emitted_to_an_integer_only_runtime_contract() {
    let failure = parse("You may play X additional lands on each of your turns.")
        .expect_err("static runtime contract requires a literal count");

    assert_eq!(failure.state, "modifier.additional-land-play.lower.count");
    assert_eq!(
        failure.expected,
        "a positive i32 literal additional-land count"
    );
}

#[test]
fn count_above_the_runtime_integer_range_is_rejected_before_ir() {
    let failure = parse("You may play 2147483648 additional lands on each of your turns.")
        .expect_err("runtime integer contract is i32-bounded");

    assert_eq!(failure.state, "modifier.additional-land-play.lower.count");
    assert_eq!(
        failure.expected,
        "a positive i32 literal additional-land count"
    );
}
