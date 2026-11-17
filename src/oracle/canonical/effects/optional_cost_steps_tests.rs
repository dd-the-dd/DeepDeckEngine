use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};

fn parse_spell(text: &str) -> EffectProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_contextual_effect_program(&AbilityInput {
        face_name: "Generic Program Source",
        face_type_line: "Sorcery",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn effect_kinds(text: &str) -> Vec<String> {
    let parsed = match parse_spell(text) {
        EffectProgramParseAttempt::Parsed(parsed) => parsed,
        EffectProgramParseAttempt::Failed(f)
        | EffectProgramParseAttempt::FailedSupport(f)
        | EffectProgramParseAttempt::Unpromoted(f) => {
            panic!(
                "expected a complete program: {text}: {} expected {}",
                f.state, f.expected
            )
        }
        _ => panic!("expected a complete program: {text}: not applicable"),
    };
    parsed.rule["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn optional_cost_after_an_ordinary_sentence_is_a_later_program_step() {
    // Pursue the Past: the leading sentence is unconditional.
    let text = "You gain 2 life. You may discard a card. If you do, draw two cards.";
    let EffectProgramParseAttempt::Parsed(parsed) = parse_spell(text) else {
        panic!("expected a complete program");
    };
    let effects = parsed.rule["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "gainLife");
    assert_eq!(effects[1]["kind"], "optionalPayCostPerformEffects");
    assert_eq!(effects[1]["cost"]["kind"], "discardCard");
    assert_eq!(effects[1]["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(effects[1]["declineEffects"], json!([]));
}

#[test]
fn outcome_governs_only_its_own_sentence_and_reminder() {
    // A new combination: leading step, cost block, then an independent step.
    assert_eq!(
        effect_kinds("Draw a card. You may pay {2}. If you do, you gain 3 life. You lose 1 life."),
        ["drawCards", "optionalPayCostPerformEffects", "loseLife"]
    );
    assert_eq!(
        effect_kinds(
            "You may sacrifice a creature. If you do, draw two cards. (This is only a reminder.)"
        ),
        ["optionalPayCostPerformEffects"]
    );
}

#[test]
fn a_second_outcome_sentence_is_not_an_ordinary_step() {
    for text in [
        "You may discard a card. If you do, draw a card. If you don't, you lose 2 life.",
        "You gain 2 life. You may discard a card. If you do, draw a card. If you don't, you lose 2 life.",
    ] {
        assert!(
            !matches!(parse_spell(text), EffectProgramParseAttempt::Parsed(_)),
            "{text}"
        );
    }
}

fn first_effect(text: &str) -> Value {
    let EffectProgramParseAttempt::Parsed(parsed) = parse_spell(text) else {
        panic!("expected a complete program: {text}");
    };
    parsed.rule["effects"][0].clone()
}

#[test]
fn blight_is_a_payable_resolution_cost_on_a_creature_you_control() {
    // Boggart Mischief shape: the reminder belongs to the outcome sentence.
    let effect = first_effect(
        "You may blight 1. If you do, draw two cards. (To blight 1, put a -1/-1 counter on a creature you control.)",
    );
    assert_eq!(effect["kind"], "optionalPayCostPerformEffects");
    assert_eq!(
        effect["cost"],
        json!({
            "kind": "putCounters", "counter": "-1/-1",
            "count": {"kind": "integer", "value": 1},
            "where": card_type("Creature"),
        })
    );
    // Gutsplitter Gang shape: a declined (or impossible) blight runs the else.
    let effect = first_effect(
        "You may blight 2. If you don't, you lose 3 life. (To blight 2, put two -1/-1 counters on a creature you control.)",
    );
    assert_eq!(
        effect["cost"]["count"],
        json!({"kind": "integer", "value": 2})
    );
    assert_eq!(effect["effects"], json!([]));
    assert_eq!(effect["declineEffects"][0]["kind"], "loseLife");
}

#[test]
fn blight_amount_must_be_a_positive_literal() {
    for text in [
        "You may blight X. If you do, draw a card.",
        "You may blight 0. If you do, draw a card.",
    ] {
        assert!(
            !matches!(parse_spell(text), EffectProgramParseAttempt::Parsed(_)),
            "{text}"
        );
    }
}
