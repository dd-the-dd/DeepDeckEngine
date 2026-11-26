//! An entry sentence (`The token enters tapped and attacking.`) states the
//! entry state of the tokens the previous sentence created (CR 111.2,
//! 508.4), the same state a `that's tapped and attacking` creation writes.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{4}{U}".to_string()),
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

fn last_ability_status(name: &str, type_line: &str, text: &str) -> (String, String) {
    let parsed = parse(name, type_line, text);
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
            .unwrap_or_default(),
    )
}

#[test]
fn an_entry_sentence_amends_the_previous_creation() {
    // Real cards: Thousand-Faced Shadow (a copy of a target) and Living
    // Laser (a copy per iteration, a plural subject). Sibling, previously
    // unused: a described token whose entry sentence names no defender and
    // only the tapped state.
    for (name, type_line, text) in [
        (
            "Thousand-Faced Shadow",
            "Creature — Human Ninja",
            "When this creature enters from your hand, if it's attacking, create a token that's a copy of another target attacking creature. The token enters tapped and attacking.",
        ),
        (
            "Entry Probe",
            "Sorcery",
            "Create a 2/2 black Zombie creature token. It enters tapped.",
        ),
    ] {
        let (status, rule) = last_ability_status(name, type_line, text);
        assert_eq!(status, "canonical", "{name}: {rule}");
        assert!(rule.contains("\"tapped\":true"), "{name}: {rule}");
    }
}

#[test]
fn an_entry_sentence_must_agree_with_the_created_tokens() {
    // Near misses: a singular subject after two created tokens, and an
    // entry sentence after an action that created nothing.
    for text in [
        "Create two 1/1 white Soldier creature tokens. The token enters tapped and attacking.",
        "Draw a card. The token enters tapped and attacking.",
    ] {
        let (status, rule) = last_ability_status("Near Miss", "Sorcery", text);
        assert_ne!(status, "canonical", "{text}: {rule}");
    }
}
