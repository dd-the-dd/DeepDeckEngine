//! `the base power and toughness of OBJECTS become VALUE` (CR 613.4b): the
//! possessed characteristic pair is the clause subject, its possessor the
//! shared permanent operand, after a causative `have` too.
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

use super::{OracleTokenCursor, base_stats_of_operand_starts};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}{U}".to_string()),
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

fn last_ability(name: &str, type_line: &str, text: &str) -> (String, String) {
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
            .unwrap_or_else(|| format!("{:?}", ability.diagnostics)),
    )
}

#[test]
fn only_the_possessed_base_pair_starts_the_frame() {
    for (text, starts) in [
        (
            "the base power and toughness of target creature become 0/2.",
            true,
        ),
        (
            "the base power and toughness of other creatures you control become 1/1.",
            true,
        ),
        // The current pair is no layer-7b change; `its` is the possessive
        // frame of a proven antecedent.
        (
            "the power and toughness of target creature become 0/2.",
            false,
        ),
        ("its base power and toughness become 0/2.", false),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("test.base-stats-of", &tokens);
        assert_eq!(base_stats_of_operand_starts(&cursor), starts, "{text}");
    }
}

#[test]
fn a_causative_base_pair_takes_the_source_characteristics() {
    // Real cards: Exuberant Wolfbear (a declared target, paired source
    // characteristics), Tanazir Quandrix (a collection, the source's own
    // name) and Unruly Krasis (a numeric pair bound by `where X`).
    for (name, type_line, text) in [
        (
            "Exuberant Wolfbear",
            "Creature — Wolf Bear",
            "Whenever this creature attacks, you may have the base power and toughness of target Human you control become equal to this creature's power and toughness until end of turn.",
        ),
        (
            "Tanazir Quandrix",
            "Legendary Creature — Elder Dragon",
            "Whenever Tanazir Quandrix attacks, you may have the base power and toughness of other creatures you control become equal to Tanazir Quandrix's power and toughness until end of turn.",
        ),
        (
            "Unruly Krasis",
            "Creature — Fish Beast",
            "Whenever this creature attacks, you may have the base power and toughness of another target creature you control become X/X until end of turn, where X is this creature's power.",
        ),
        // Unused combination: an imperative-free sentence over a universal
        // collection with a literal pair.
        (
            "Base Probe",
            "Sorcery",
            "The base power and toughness of each creature your opponents control become 1/1 until end of turn.",
        ),
    ] {
        let (status, rule) = last_ability(name, type_line, text);
        assert_eq!(status, "canonical", "{name}: {rule}");
    }
}

#[test]
fn a_base_pair_needs_the_plural_verb_and_a_value() {
    // Near misses: a singular verb after the plural pair, and a pair of
    // another object's characteristics joined to a toughness.
    for text in [
        "The base power and toughness of target creature becomes 0/2 until end of turn.",
        "The base power and toughness of target creature become equal to the number of cards in your hand and toughness until end of turn.",
    ] {
        let (status, rule) = last_ability("Near Miss", "Instant", text);
        assert_ne!(status, "canonical", "{text}: {rule}");
    }
}
