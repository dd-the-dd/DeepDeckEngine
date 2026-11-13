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
        face_name: "Generic Activated Move",
        face_type_line: "Land",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn lowers_return_after_shared_activation_costs() {
    let text = "{2}, {T}, Sacrifice this land: Return target artifact card from your graveyard to your hand.";
    assert!(with_input(
        text,
        claims_promoted_activated_card_zone_change_family
    ));
    let rule = with_input(text, parse_promoted_activated_card_zone_change)
        .expect("activated return should parse")
        .rule;

    assert_eq!(rule["costs"][0]["kind"], "payMana");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["costs"][2]["kind"], "sacrificePermanent");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(rule["effects"][0]["kind"], "moveCards");
    assert_eq!(rule["effects"][0]["from"]["kind"], "graveyard");
    assert_eq!(rule["effects"][0]["to"]["kind"], "hand");
}

#[test]
fn action_words_inside_quotes_do_not_open_the_activated_move_family() {
    let text = "{2}, Discard a card named \"Return target card\": Add {G}.";
    assert!(!with_input(
        text,
        claims_promoted_activated_card_zone_change_family
    ));
}

#[test]
fn malformed_return_after_the_root_colon_reports_the_child_state() {
    let text = "{2}, {T}: Return target artifact card from your graveyard your hand.";
    assert!(with_input(
        text,
        claims_promoted_activated_card_zone_change_family
    ));
    let failure = with_input(text, parse_promoted_activated_card_zone_change)
        .expect_err("a claimed malformed destination must not fall back");

    assert_eq!(
        failure.state,
        "effect.target-card-move.return.destination-relation"
    );
}

#[test]
fn permanent_return_to_owner_hand_remains_outside_the_card_move_family() {
    let text = "{T}: Return target legendary creature to its owner's hand.";

    assert!(!with_input(
        text,
        claims_promoted_activated_card_zone_change_family
    ));
}
