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
        face_name: "Generalized Entry Source",
        face_type_line: "Creature — Bird",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    with_input(text, parse_promoted_source_entry_effect_sequence)
}

fn claims(text: &str) -> bool {
    with_input(text, claims_promoted_source_entry_effect_sequence_family)
}

#[test]
fn lowers_source_entry_draw_as_a_primitive_triggered_sequence() {
    let rule = parse("When this creature enters, draw a card.")
        .expect("source entry plus draw should lower natively")
        .rule;

    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(rule["event"]["kind"], "enterBattlefield");
    assert_eq!(rule["event"]["object"]["kind"], "self");
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
    assert_eq!(rule["effects"][0]["count"]["value"], 1);
    assert!(rule.get("declaration").is_none());
}

#[test]
fn lowers_targeted_modifier_and_untap_with_one_shared_slot() {
    let rule =
        parse("When this creature enters, target creature gets +1/+1 until end of turn. Untap it.")
            .expect("the source-entry parent should preserve sequence declarations")
            .rule;

    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
    assert_eq!(rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][0]["object"]["id"], "targetPermanent");
    assert_eq!(rule["effects"][1]["kind"], "untapPermanent");
    assert_eq!(rule["effects"][1]["permanent"]["id"], "targetPermanent");
}

#[test]
fn optional_return_declares_one_required_target_and_wraps_only_resolution() {
    let text =
        "When this creature enters, you may return target card from your graveyard to your hand.";
    assert!(claims(text));
    let rule = parse(text)
        .expect("the shared source-entry parent should lower the optional card move")
        .rule;

    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "targetCard");
    assert_eq!(decision["minimum"], 1);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["zone"]["kind"], "graveyard");
    assert_eq!(rule["effects"][0]["kind"], "optionalEffects");
    assert_eq!(rule["effects"][0]["effects"][0]["kind"], "moveCards");
    assert_eq!(
        rule["effects"][0]["effects"][0]["cards"]["id"],
        "targetCard"
    );
}

#[test]
fn malformed_optional_return_is_claimed_and_reports_its_local_zone_state() {
    let text =
        "When this creature enters, you may return target card from your graveyard your hand.";
    assert!(claims(text));
    let failure = parse(text).expect_err("the promoted near miss must not fall back");

    assert_eq!(
        failure.state,
        "effect.target-card-move.return.destination-relation"
    );
    assert_eq!(failure.actual.as_deref(), Some("your"));
}

#[test]
fn optional_return_does_not_make_a_following_draw_optional() {
    let text = "When this creature enters, you may return target card from your graveyard to your hand. Draw a card.";
    assert!(claims(text));
    let failure = parse(text)
        .expect_err("the parent must reject a sequence whose optional scope is ambiguous");

    assert_eq!(
        failure.state,
        "trigger.source-entry-sequence.optional-scope"
    );
}

#[test]
fn malformed_promoted_draw_is_claimed_and_fails_in_its_child_machine() {
    let text = "When this creature enters, draw target player a card.";
    assert!(claims(text));
    let failure = parse(text).expect_err("the claimed malformed draw must remain visible");

    assert_eq!(failure.state, "numeric.atom.start");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}

#[test]
fn unrelated_or_unpromoted_entry_bodies_remain_outside_the_boundary() {
    assert!(!claims("Whenever a creature enters, draw a card."));
    assert!(!claims(
        "When this creature enters, target player draws a card."
    ));
}
