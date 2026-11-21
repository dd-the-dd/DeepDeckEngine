use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_entities, resolve_counter_name_spans},
};

fn attempt(text: &str, type_line: &str) -> LifeGainReplacementParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    try_parse_life_gain_replacement(&AbilityInput {
        face_name: "Synthetic life replacement",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn modifier(text: &str) -> Value {
    match attempt(text, "Enchantment") {
        LifeGainReplacementParseAttempt::Parsed(draft) => draft.rule["modifiers"][0].clone(),
        LifeGainReplacementParseAttempt::Failed(failure) => panic!("{text}: {failure:?}"),
        LifeGainReplacementParseAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn event_quantity() -> Value {
    json!({ "kind": "eventQuantity" })
}

#[test]
fn life_gain_amounts_reuse_the_event_quantity_arithmetic() {
    // Angel of Vitality and Boon Reflection.
    assert_eq!(
        modifier("If you would gain life, you gain that much life plus 1 instead."),
        json!({
            "kind": "replaceLifeGain",
            "event": { "player": controller() },
            "quantity": {
                "kind": "add",
                "left": event_quantity(),
                "right": { "kind": "integer", "value": 1 },
            },
        })
    );
    assert_eq!(
        modifier("If you would gain life, you gain twice that much life instead.")["quantity"],
        json!({ "kind": "multiply", "left": { "kind": "integer", "value": 2 }, "right": event_quantity() })
    );
    // Unused combination: an opponent's gain, halved and rounded down.
    assert_eq!(
        modifier(
            "If an opponent would gain life, that player gains half that much life, rounded down, instead."
        ),
        json!({
            "kind": "replaceLifeGain",
            "event": { "player": { "kind": "opponentsOf", "player": controller() } },
            "quantity": {
                "kind": "divide",
                "left": event_quantity(),
                "right": { "kind": "integer", "value": 2 },
                "round": "down",
            },
        })
    );
}

#[test]
fn life_gain_near_misses_keep_precise_failures() {
    for (text, state) in [
        // A different replacing action (CR 614.1a is still a replacement, but
        // drawing is not this life-gain modification).
        (
            "If you would gain life, draw that many cards instead.",
            "replacement.life-gain.subject",
        ),
        // The replacing subject must restate the affected player.
        (
            "If an opponent would gain life, you gain that much life instead.",
            "replacement.life-gain.subject",
        ),
        // A state condition between the event and the comma is unsupported.
        (
            "If you would gain life while you have 5 or less life, you gain twice that much life instead.",
            "replacement.life-gain.separator",
        ),
    ] {
        match attempt(text, "Enchantment") {
            LifeGainReplacementParseAttempt::Failed(failure) => {
                assert_eq!(failure.state, state, "{text}")
            }
            _ => panic!("{text}: expected a committed failure"),
        }
    }
    assert!(matches!(
        attempt("If you control a creature, you gain 1 life.", "Enchantment"),
        LifeGainReplacementParseAttempt::NotClaimed
    ));
    assert!(matches!(
        attempt(
            "If you would gain life, you gain that much life plus 1 instead.",
            "Sorcery"
        ),
        LifeGainReplacementParseAttempt::Failed(_)
    ));
}
