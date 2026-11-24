//! A keyword line that no reader completes names the item the shared keyword
//! reader rejects, or the separator when every item reads alone (CR 702.1).
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn failure(text: &str) -> (String, String) {
    let parsed: OracleCardParseResult = parse_oracle_card(OracleCardParseRequest {
        card_name: "Keyword List Probe".to_string(),
        type_line: "Creature — Human Soldier".to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "unsupported", "{text}");
    let diagnostic = &parsed.diagnostics[0];
    (
        diagnostic.state.clone().unwrap_or_default(),
        diagnostic.found.clone().unwrap_or_default(),
    )
}

#[test]
fn banding_after_first_strike_is_the_reported_item() {
    // Pikemen, real Oracle text.
    let (state, found) = failure(
        "First strike; banding (Any creatures with banding, and up to one without, can attack in a band. Bands are blocked as a group. If any creatures with banding you control are blocking or being blocked by a creature, you divide that creature's combat damage, not its controller, among any of the creatures it's being blocked by or is blocking.)",
    );
    assert_eq!(
        (state.as_str(), found.as_str()),
        ("ability-frame.keyword.expansion", "banding")
    );
}

#[test]
fn a_later_parameterized_item_or_the_separator_is_reported() {
    // The hexproof-from reader owns its quality and names the one it rejects.
    let (state, found) = failure("Flying, hexproof from multicolored");
    assert_eq!(state, "keyword.protection.quality");
    assert_eq!(found, "multicolored");
    // Every item reads alone: the `;` list separator is what is missing.
    let (state, found) = failure(
        "Haste; horsemanship (This creature can't be blocked except by creatures with horsemanship.)",
    );
    assert_eq!(
        (state.as_str(), found.as_str()),
        ("ability-frame.keyword.list-separator", ";")
    );
}

#[test]
fn a_single_unsupported_keyword_is_still_reported_itself() {
    let (state, found) = failure(
        // A keyword without a template row or reader (rampage now has one).
        "Absorb 2 (If a source would deal damage to this creature, prevent 2 of that damage.)",
    );
    assert_eq!(
        (state.as_str(), found.as_str()),
        ("ability-frame.keyword.expansion", "Absorb")
    );
}
