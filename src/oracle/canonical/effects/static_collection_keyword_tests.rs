use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

fn with_input<T>(text: &str, type_line: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    operation(&AbilityInput {
        face_name: "Synthetic Collection Grant",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn static_adapter_does_not_claim_conditional_temporary_or_nonpermanent_envelopes() {
    for (text, type_line) in [
        (
            "If you control an Island, creatures you control have flying.",
            "Enchantment",
        ),
        (
            "Creatures you control have flying as long as you control an Island.",
            "Enchantment",
        ),
        (
            "Creatures you control have haste until end of turn.",
            "Enchantment",
        ),
        (
            "Creatures you control have flying or vigilance.",
            "Enchantment",
        ),
        (
            "Creatures you control have haste. You gain 1 life.",
            "Enchantment",
        ),
        ("Creatures you control have haste.", "Instant"),
        (
            "{T}: Creatures you control gain haste until end of turn.",
            "Artifact",
        ),
        (
            "Whenever a creature enters, creatures you control gain haste until end of turn.",
            "Enchantment",
        ),
    ] {
        with_input(text, type_line, |input| {
            assert!(
                matches!(
                    try_parse_promoted_static_collection_keyword_grant(input),
                    StaticCollectionKeywordParseAttempt::NotClaimed
                ),
                "must retain its own envelope: {text}"
            );
        });
    }
}

#[test]
fn a_card_domain_is_not_silently_changed_to_a_permanent_collection() {
    with_input(
        "Creature cards you control have haste.",
        "Enchantment",
        |input| {
            let StaticCollectionKeywordParseAttempt::Failed(failure) =
                try_parse_promoted_static_collection_keyword_grant(input)
            else {
                panic!("a card collection needs an independently supported zone");
            };
            assert!(failure.expected.contains("battlefield-permanent"));
        },
    );
}
