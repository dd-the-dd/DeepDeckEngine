//! `SET each deal damage equal to their power to RECIPIENT` (CR 608.2c): a
//! plural declared subject, or `they` naming the plural slot the previous
//! action declared, performs the ordinary damage action once per member.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

fn status_and_rule(name: &str, text: &str) -> (String, String) {
    let parsed = parse(name, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    (
        ability.status.clone(),
        ability
            .rule
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("{:?}", ability.diagnostics)),
    )
}

#[test]
fn each_member_of_a_plural_subject_deals_its_own_damage() {
    // Real cards: Allies at Last and Band Together (a declared plural
    // subject), Terrific Team-Up and Coordinated Clobbering (`they` after an
    // action on the declared slot). Unused combination: `they` after a
    // counter placement, with a player recipient.
    for (name, text) in [
        (
            "Allies at Last",
            "Up to two target creatures you control each deal damage equal to their power to target creature an opponent controls.",
        ),
        (
            "Band Together",
            "Up to two target creatures you control each deal damage equal to their power to another target creature.",
        ),
        (
            "Terrific Team-Up",
            "One or two target creatures you control each get +1/+0 until end of turn. They each deal damage equal to their power to target creature an opponent controls.",
        ),
        (
            "Coordinated Clobbering",
            "Tap one or two target untapped creatures you control. They each deal damage equal to their power to target creature an opponent controls.",
        ),
        (
            "Distributive Probe",
            "Put a +1/+1 counter on each of up to two target creatures you control. They each deal damage equal to their power to target opponent.",
        ),
    ] {
        let (status, rule) = status_and_rule(name, text);
        assert_eq!(status, "canonical", "{name}: {rule}");
        assert!(rule.contains("\"forEachObject\""), "{name}: {rule}");
    }
}

#[test]
fn a_distributive_predicate_needs_a_plural_member_set() {
    // Near misses: a singular declared subject has no members to distribute
    // over, and `they` without a declared plural slot names no set.
    for text in [
        "Target creature you control each deal damage equal to their power to target creature an opponent controls.",
        "Draw a card. They each deal damage equal to their power to target creature an opponent controls.",
    ] {
        let (status, rule) = status_and_rule("Near Miss", text);
        assert_ne!(status, "canonical", "{text}: {rule}");
    }
}
