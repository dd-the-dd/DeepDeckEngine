use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn input<T>(text: &str, f: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_oracle_entities(text);
    let catalog =
        ExactNameTokenIndex::from_names(crate::card_catalog::named_token_names().unwrap());
    resolve_named_token_spans(&mut tokens, &catalog);
    f(&AbilityInput {
        face_name: "Generic Source",
        face_type_line: "Sorcery",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}
fn program(text: &str) -> Value {
    input(text, |i| {
        let EffectProgramParseAttempt::Parsed(draft) = try_parse_contextual_effect_program(i)
        else {
            panic!("expected composed program: {text}");
        };
        draft.rule
    })
}
#[test]
fn scry_is_an_atomic_child_before_between_and_after_other_actions() {
    for text in [
        "Scry 2, then draw a card.",
        "You gain 1 life and scry 2, then draw a card.",
        "Draw a card, then scry 2.",
    ] {
        let rule = program(text);
        let effects = rule["effects"].as_array().unwrap();
        assert!(effects.iter().any(|e| e["kind"] == "lookAtTopCards"));
        assert!(effects.iter().any(|e| e["kind"] == "drawCards"));
        assert!(
            effects
                .iter()
                .any(|e| e["kind"] == "emitAbilitySignal" && e["abilityId"] == "scry")
        );
    }
}
#[test]
fn family_uses_typed_deferral_without_claiming_the_sequence() {
    assert!(input("Scry 2.", |i| matches!(
        try_parse_promoted_scry_family(i),
        ScryFamilyParseAttempt::Parsed(_)
    )));
    for text in ["Scry 2, then draw a card.", "Scry 2. Draw a card."] {
        assert!(input(text, |i| matches!(
            try_parse_promoted_scry_family(i),
            ScryFamilyParseAttempt::DeferredToProgram
        )));
    }
}
#[test]
fn composed_reminder_validates_both_numeric_mentions_and_optional_zone_qualifier() {
    for prefix in ["", "To scry 2, "] {
        for zone in ["", " of your library"] {
            let text = format!(
                "Scry 2, then draw a card. ({prefix}Look at the top two cards of your library, then put any number of them on the bottom{zone} and the rest on top in any order.)"
            );
            let rule = program(&text);
            let effects = rule["effects"].as_array().unwrap();
            let signal = effects
                .iter()
                .position(|e| e["abilityId"] == "scry")
                .unwrap();
            let draw = effects
                .iter()
                .position(|e| e["kind"] == "drawCards")
                .unwrap();
            assert!(signal < draw);
        }
    }
}
#[test]
fn mismatched_reminder_and_unconsumed_suffix_do_not_become_coverage() {
    for text in [
        "Scry 2, then draw a card. (To scry 3, look at the top two cards of your library, then put any number of them on the bottom and the rest on top in any order.)",
        "Scry 2, then draw a card. (Look at the top three cards of your library, then put any number of them on the bottom and the rest on top in any order.)",
        "Scry 2, then.",
        "Scry 2, then teleport a card.",
    ] {
        assert!(
            input(text, |i| !matches!(
                try_parse_contextual_effect_program(i),
                EffectProgramParseAttempt::Parsed(_)
            )),
            "{text}"
        );
    }
}

#[test]
fn a_named_token_reminder_is_not_captured_by_an_earlier_scry() {
    let rule = program(
        "Scry 1, then create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain three life.\")",
    );
    assert!(
        rule["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "createTokens")
    );
}
#[test]
fn an_unlabelled_reminder_does_not_skip_a_later_partition_action() {
    let text = "Scry 1, then surveil 2. (Look at the top card of your library. You may put that card on the bottom.)";
    assert!(input(text, |i| !matches!(
        try_parse_contextual_effect_program(i),
        EffectProgramParseAttempt::Parsed(_)
    )));
}
