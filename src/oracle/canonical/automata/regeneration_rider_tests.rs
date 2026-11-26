//! A sentence-level regeneration rider (CR 701.19c) amends the destruction
//! of the sentence before it when the destroy reader could not read the
//! rider as its own amendment: after a postposed `where X`, after a bound
//! reference destroyed by a trigger, or restated by `destroyed this way`.
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

use super::{OracleTokenCursor, regeneration_rider_starts};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Rider Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{B}".to_string()),
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

fn canonical_rendering(type_line: &str, text: &str) -> String {
    let parsed = parse(type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_eq!(ability.status, "canonical", "{text}: {parsed:#?}");
    ability.rule.clone().expect("canonical rule").to_string()
}

#[test]
fn only_an_undated_rider_after_a_sentence_end_starts() {
    for (text, starts) in [
        ("They can't be regenerated.", true),
        ("The creature can't be regenerated.", true),
        ("A creature destroyed this way can't be regenerated.", true),
        ("Creatures destroyed this way can't be regenerated.", true),
        // A duration makes it a lasting rule, read by the object frames.
        ("It can't be regenerated this turn.", false),
        (
            "A creature dealt damage this way can't be regenerated this turn.",
            false,
        ),
        ("They can't block this turn.", false),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("test.regeneration-rider", &tokens);
        assert_eq!(regeneration_rider_starts(&cursor), starts, "{text}");
    }
}

#[test]
fn a_rider_amends_the_destruction_of_the_previous_sentence() {
    // Real cards: Vile Requiem (after a postposed `where X`), Mephitic Ooze
    // (a bound reference destroyed by a trigger) and War Barge (a restated
    // noun inside a delayed trigger).
    for (type_line, text) in [
        (
            "Enchantment",
            "{1}{B}, Sacrifice this enchantment: Destroy up to X target nonblack creatures, where X is the number of verse counters on this enchantment. They can't be regenerated.",
        ),
        (
            "Creature — Ooze",
            "Whenever this creature deals combat damage to a creature, destroy that creature. The creature can't be regenerated.",
        ),
        (
            "Artifact",
            "{3}: Target creature gains islandwalk until end of turn. When this artifact leaves the battlefield this turn, destroy that creature. A creature destroyed this way can't be regenerated.",
        ),
        // Unused combination: a nominal rider after a bound blocker the
        // trigger destroys.
        (
            "Creature — Wall",
            "Whenever this creature becomes blocked by a creature, destroy that creature. That creature can't be regenerated.",
        ),
    ] {
        let rendered = canonical_rendering(type_line, text);
        assert!(
            rendered.contains("\"cannotRegenerate\":true"),
            "{text}: {rendered}"
        );
    }
}

#[test]
fn a_rider_must_agree_with_the_destruction_it_amends() {
    // Near misses: a plural rider after one destroyed target, and a rider
    // after an action that destroys nothing.
    for (type_line, text) in [
        (
            "Instant",
            "Destroy target creature, where X is 2. They can't be regenerated.",
        ),
        ("Instant", "Tap target creature. It can't be regenerated."),
    ] {
        let parsed = parse(type_line, text);
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.status != "canonical"),
            "{text}: {parsed:#?}"
        );
    }
}
