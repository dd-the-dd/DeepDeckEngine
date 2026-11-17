use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

const EXACT: &str =
    "If this card is in your opening hand, you may begin the game with it on the battlefield.";

fn source(text: &str) -> AbilitySource {
    AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    }
}

fn claims(text: &str) -> bool {
    let source = source(text);
    let tokens = recognize_oracle_entities(text);
    claims_promoted_opening_hand_battlefield_family(&AbilityInput {
        face_name: "Generic Opening-Hand Permanent",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = source(text);
    let tokens = recognize_oracle_entities(text);
    parse_promoted_opening_hand_battlefield(&AbilityInput {
        face_name: "Generic Opening-Hand Permanent",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn exact_shape_lowers_to_the_existing_pregame_primitive() {
    assert!(claims(EXACT));
    let rule = parse(EXACT).expect("the exact generic shape lowers").rule;

    assert_eq!(rule["kind"], "keywordAbility");
    assert_eq!(rule["source"]["kind"], "self");
    assert_eq!(rule["ability"]["kind"], "openingHandBattlefield");
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn claimed_destination_near_miss_reports_the_child_state() {
    let text =
        "If this card is in your opening hand, you may begin the game with it in your graveyard.";
    assert!(claims(text));

    let failure = parse(text).expect_err("a claimed near miss must not use legacy fallback");

    assert_eq!(failure.automaton, "opening-hand-battlefield");
    assert_eq!(failure.state, "pregame.opening-hand.destination-relation");
    assert_eq!(failure.actual.as_deref(), Some("in"));
}

#[test]
fn unrelated_opening_hand_condition_is_not_claimed() {
    assert!(!claims(
        "If this card is in your opening hand, you may reveal it."
    ));
}
