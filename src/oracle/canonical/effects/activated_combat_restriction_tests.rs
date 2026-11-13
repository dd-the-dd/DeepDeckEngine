use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::{
    ActivatedCombatRestrictionParseAttempt, try_parse_promoted_activated_combat_restriction,
};

fn parse(text: &str) -> ActivatedCombatRestrictionParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_promoted_activated_combat_restriction(&AbilityInput {
        face_name: "Generic Combat Restriction Source",
        face_type_line: "Land",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn lowers_cost_target_restriction_and_duration_as_primitives() {
    let ActivatedCombatRestrictionParseAttempt::Parsed(parsed) =
        parse("{4}, {T}: Target creature can't be blocked this turn.")
    else {
        panic!("a complete activated combat restriction must parse");
    };
    assert_eq!(parsed.rule["costs"][0]["manaCost"], "{4}");
    assert_eq!(parsed.rule["costs"][1]["kind"], "tap");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "targetPermanent"
    );
    assert_eq!(parsed.rule["effects"][0]["keyword"], "cantBeBlocked");
    assert_eq!(
        parsed.rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
}

#[test]
fn typed_commit_keeps_malformed_combat_duration_and_rejects_noncombat_candidates() {
    let ActivatedCombatRestrictionParseAttempt::Failed(failure) =
        parse("{2}, {T}: Target creature can't be blocked this combat.")
    else {
        panic!("a committed malformed duration must remain a hard failure");
    };
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");

    for outside_family in [
        "{2}, {T}: Target creature can't activate abilities this turn.",
        "{2}, {T}: Target creature can't be sacrificed this turn.",
        "{2}, {T}: Target creature can't be restored this turn.",
        "Target creature gains \"{T}: Target creature can't block this turn.\"",
    ] {
        assert!(matches!(
            parse(outside_family),
            ActivatedCombatRestrictionParseAttempt::NotClaimed
        ));
    }
}

#[test]
fn unsupported_costs_and_trailing_programs_return_a_structured_unpromoted_result() {
    for outside_native_scope in [
        "{R}, Sacrifice an artifact: Target creature can't block this turn.",
        "{T}: Target creature can't be blocked this turn. Activate only as a sorcery.",
        "{G}{U}: Target creature can't be blocked this turn. If that creature is a Snake, it gets +2/+2 until end of turn.",
    ] {
        assert!(matches!(
            parse(outside_native_scope),
            ActivatedCombatRestrictionParseAttempt::Unpromoted(_)
        ));
    }
}

#[test]
fn unsupported_target_collection_keeps_its_lowering_diagnostic() {
    let ActivatedCombatRestrictionParseAttempt::Unpromoted(failure) =
        parse("{4}, {T}: Two target creatures can't be blocked this turn.")
    else {
        panic!("a target collection requires collection-aware lowering");
    };
    assert_eq!(failure.state, "lower.keyword-grant.scalar-target");
}

#[test]
fn tentative_selector_failure_is_preserved_without_a_committed_promotion() {
    let ActivatedCombatRestrictionParseAttempt::Unpromoted(failure) =
        parse("{T}: Target creature with power enormous can't be blocked this turn.")
    else {
        panic!("an unsupported numeric threshold must remain unpromoted");
    };
    assert_eq!(failure.state, "numeric.atom.start");
}
