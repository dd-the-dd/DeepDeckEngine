use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};

fn parse_activation(type_line: &str, text: &str) -> EffectProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_contextual_effect_program(&AbilityInput {
        face_name: "Generic Program Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let EffectProgramParseAttempt::Parsed(parsed) = parse_activation(type_line, text) else {
        panic!("expected a complete activated program: {text}");
    };
    parsed.rule
}

#[test]
fn tapping_n_untapped_permanents_is_n_declared_object_costs() {
    let rule = parsed_rule(
        "Creature - Human Wizard",
        "Tap two untapped artifacts you control: Put two +1/+1 counters on this creature.",
    );
    assert_eq!(rule["kind"], "activatedAbility");
    let taps = rule["costs"].as_array().unwrap();
    assert_eq!(taps.len(), 2);
    let choice = &rule["declaration"]["decisions"][0];
    assert_eq!(choice["kind"], "chooseObjects");
    assert_eq!(choice["candidates"]["kind"], "permanents");
    assert_eq!(choice["quantity"]["value"], integer(2));
    // CR 118.3: each chosen permanent is tapped; ids follow the engine's
    // `id:index` keys for a multi-object choice.
    let id = choice["id"].as_str().unwrap();
    for (index, tap) in taps.iter().enumerate() {
        assert_eq!(tap["kind"], "tap");
        assert_eq!(tap["object"]["id"], format!("{id}:{index}"));
    }
}

#[test]
fn tap_objects_composes_with_other_costs_and_a_single_choice() {
    // Previously unused combination: mana + one chosen creature + a draw.
    let rule = parsed_rule(
        "Artifact",
        "{1}, Tap an untapped creature you control: Draw a card.",
    );
    assert_eq!(rule["costs"][1]["kind"], "tap");
    let choice = &rule["declaration"]["decisions"][0];
    assert_eq!(rule["costs"][1]["object"]["id"], choice["id"]);
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
}

#[test]
fn activation_tap_cost_failures_are_explicit() {
    // `another` is now a declared source exclusion; an unscoped tapped
    // permanent still names no payer-controlled domain.
    let attempt = parse_activation("Creature", "Tap another untapped creature: Draw a card.");
    let (EffectProgramParseAttempt::Failed(failure)
    | EffectProgramParseAttempt::FailedSupport(failure)
    | EffectProgramParseAttempt::Unpromoted(failure)) = attempt
    else {
        panic!("an explicit grammar failure is required");
    };
    assert_eq!(failure.state, "cost.payment.tap.domain");
}
