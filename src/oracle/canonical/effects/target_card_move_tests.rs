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
        face_name: "Generic Zone Move",
        face_type_line: "Instant",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    with_input(text, parse_promoted_target_card_move_spell)
}

#[test]
fn lowers_any_graveyard_target_to_its_owner_library_top() {
    let text = "Put target card from a graveyard on top of its owner's library.";
    assert!(with_input(
        text,
        claims_promoted_target_card_move_spell_family
    ));
    let rule = parse(text).expect("targeted card move should parse").rule;

    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 1);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["zone"]["kind"], "anyGraveyard");
    assert_eq!(rule["effects"][0]["kind"], "moveCards");
    assert!(rule["effects"][0].get("from").is_none());
    assert_eq!(rule["effects"][0]["to"]["kind"], "library");
    assert_eq!(rule["effects"][0]["to"]["position"], "top");
    assert_eq!(rule["effects"][0]["to"]["player"]["kind"], "ownerOf");
}

#[test]
fn card_type_sibling_reuses_the_shared_selector() {
    let rule = parse(
        "Put target instant or sorcery card from a graveyard on the top of its owner’s library.",
    )
    .expect("typed sibling should parse")
    .rule;

    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );
}

#[test]
fn return_spell_reuses_the_same_move_parent_and_controller_zones() {
    let text = "Return target creature card from your graveyard to your hand.";
    assert!(with_input(
        text,
        claims_promoted_target_card_move_spell_family
    ));
    let rule = parse(text).expect("return spell sibling should parse").rule;

    assert_eq!(rule["effects"][0]["kind"], "moveCards");
    assert_eq!(rule["effects"][0]["from"]["kind"], "graveyard");
    assert_eq!(rule["effects"][0]["to"]["kind"], "hand");
}

#[test]
fn malformed_owner_relation_remains_a_committed_machine_failure() {
    let text = "Put target card from a graveyard on top of its library.";
    assert!(with_input(
        text,
        claims_promoted_target_card_move_spell_family
    ));
    let failure = parse(text).expect_err("owner relation is required");

    assert_eq!(
        failure.state,
        "effect.target-card-move.put.destination-owner"
    );
    assert_eq!(failure.actual.as_deref(), Some("library"));
}

#[test]
fn battlefield_reanimation_remains_outside_the_library_top_family() {
    let text = "Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value.";

    assert!(!with_input(
        text,
        claims_promoted_target_card_move_spell_family
    ));
}

#[test]
fn unsupported_source_zone_is_not_approximated_as_a_graveyard() {
    let text = "Put target card from exile on top of its owner's library.";

    assert!(!with_input(
        text,
        claims_promoted_target_card_move_spell_family
    ));
}
