use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

fn parse(text: &str, face_type_line: &str) -> StaticSourceCombatRestrictionAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_static_source_combat_restriction(&AbilityInput {
        face_name: "Generic Restriction",
        face_type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn independent_restrictions_keep_their_existing_runtime_markers() {
    for (text, marker) in [
        ("This creature can't block.", "cantBlock"),
        ("This creature can't attack.", "cantAttack"),
        ("This creature can\u{2019}t be blocked.", "cantBeBlocked"),
    ] {
        let StaticSourceCombatRestrictionAttempt::Parsed(parsed) = parse(text, "Creature") else {
            panic!("expected source restriction: {text}");
        };
        assert_eq!(parsed.rule["modifiers"][0]["keyword"], marker);
        assert_ne!(parsed.rule["modifiers"][0]["keyword"], "defender");
    }
}

#[test]
fn full_envelope_prevents_atomic_claim_of_other_families() {
    for text in [
        "This creature can't block this turn.",
        "This creature can't block unless you control an Island.",
        "This creature can't block as long as you control an Island.",
        "This creature can't block. You gain 1 life.",
        "If you control an Island, this creature can't block.",
        "This creature can't attack or block.",
        "This creature can't be blocked by red creatures.",
    ] {
        assert!(
            matches!(
                parse(text, "Creature"),
                StaticSourceCombatRestrictionAttempt::NotClaimed
            ),
            "{text}"
        );
    }
    assert!(matches!(
        parse("This spell can't block.", "Instant"),
        StaticSourceCombatRestrictionAttempt::NotClaimed
    ));
}
