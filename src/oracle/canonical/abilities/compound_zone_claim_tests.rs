use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::recognize_oracle_entities;

fn with_input<T>(text: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    operation(&AbilityInput {
        face_name: "Generic Atomic Source",
        face_type_line: "Artifact",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn target_head_is_shared_with_the_target_child_and_excludes_collections() {
    for (text, expected) in [
        ("target creature", true),
        ("another target creature", true),
        ("up to one other target creature", true),
        ("two target creatures", true),
        ("all graveyards", false),
        ("each opponent's graveyard", false),
        ("a creature", false),
    ] {
        let tokens = recognize_oracle_entities(text);
        let cursor = OracleTokenCursor::new("shared-target-head-test", &tokens);
        assert_eq!(starts_explicit_target_slot(&cursor), expected, "{text}");
    }
}

#[test]
fn compound_claim_does_not_intercept_collections_conditional_or_nested_effects() {
    for text in [
        "{T}, Sacrifice this artifact: Exile each opponent's graveyard.",
        "{T}, Sacrifice this artifact: Exile all graveyards.",
        "{T}: If you control a creature, exile target artifact.",
        "{T}: Creatures gain \"{T}: Exile target artifact.\"",
        "Sacrifice this creature: Draw a card. Exile target artifact.",
    ] {
        assert!(
            with_input(text, |input| matches!(
                try_parse_promoted_compound_zone_change_activation(input),
                CompoundZoneChangeActivationParseAttempt::NotClaimed
            )),
            "{text}"
        );
    }
}

#[test]
fn a_targeted_prefix_with_more_effects_is_unpromoted_not_a_hard_atomic_failure() {
    for text in [
        "{T}: Exile target creature. Draw a card.",
        "{T}: Exile target creature and draw a card.",
    ] {
        assert!(
            with_input(text, |input| matches!(
                try_parse_promoted_compound_zone_change_activation(input),
                CompoundZoneChangeActivationParseAttempt::Unpromoted(_)
            )),
            "{text}"
        );
    }
}

#[test]
fn missing_colon_remains_a_committed_known_family_failure() {
    with_input(
        "{T}, Sacrifice this land Destroy target nonbasic land.",
        |input| {
            let CompoundZoneChangeActivationParseAttempt::Failed(failure) =
                try_parse_promoted_compound_zone_change_activation(input)
            else {
                panic!("missing colon in a known targeted family must be diagnosed");
            };
            assert_eq!(failure.automaton, "compound-zone-change-activation");
            assert_eq!(failure.state, "activation.compound.effect-separator");
        },
    );
}
