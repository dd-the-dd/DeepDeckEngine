use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn with_input<T>(text: &str, run: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let tokens = recognize_entities(text);
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    run(&AbilityInput {
        face_name: "Contextual Source",
        face_type_line: "Sorcery",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn all_new_population_programs_defer_before_compatibility_declaration_lowering() {
    for text in [
        "This spell deals X damage to each opponent.",
        "This spell deals X damage to each opponent and you gain X life.",
        "This spell deals 2 damage to each player and you gain X life.",
    ] {
        with_input(text, |input| {
            assert!(matches!(
                try_parse_promoted_damage_family(input, "compatibility-deferral"),
                DamageFamilyParseAttempt::DeferredToContextualProgram
            ));
            let EffectProgramParseAttempt::Parsed(draft) =
                try_parse_contextual_effect_program(input)
            else {
                panic!(
                    "contextual child recognizes reference; composition validates actual producer"
                )
            };
            assert!(
                draft.rule.get("declaration").is_none(),
                "X use must not create chooseNumber: {}",
                draft.rule
            );
        });
    }
}

#[test]
fn historical_scalar_compatibility_keeps_its_ir_and_producer_behavior_for_separate_review() {
    with_input("This spell deals X damage to you.", |input| {
        let DamageFamilyParseAttempt::Parsed(draft) =
            try_parse_promoted_damage_family(input, "plain-damage-effect")
        else {
            panic!("old scalar contract retained")
        };
        assert_eq!(draft.rule["declaration"]["decisions"][0]["id"], "xValue");
        assert_eq!(draft.rule["effects"][0]["recipient"], controller());
    });
}
