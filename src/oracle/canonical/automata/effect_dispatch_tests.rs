//! The effect dispatcher (S3): heads are `SUBJECT x LEMMA x MODALITY`, a
//! declared subject fails naming its verb instead of a missing player, and a
//! postposed `if` is the same conditional as the preposed one (#11).
use serde_json::Value;

use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

use super::super::{LocalEffectNode, recognize_effect_sequence};
use super::OracleTokenCursor;

fn sequence_failure(text: &str) -> super::super::super::MachineFailure {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.effect-dispatch", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None).expect_err(text)
}

fn sequence_effects(text: &str) -> Vec<LocalEffectNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.effect-dispatch", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None)
        .unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
        .effects
}

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

/// The frame an effect node belongs to, for the dispatch table below.
fn frame_of(effect: &LocalEffectNode) -> &'static str {
    match effect {
        LocalEffectNode::ZoneChange(_) => "zone-change",
        LocalEffectNode::Scry(_) => "scry",
        LocalEffectNode::Investigate(_) => "investigate",
        LocalEffectNode::TapPermanents(_) => "tap",
        LocalEffectNode::AddMana(_) => "add-mana",
        _ => "other",
    }
}

#[test]
fn every_imperative_lemma_keeps_its_frame() {
    for (text, frame) in [
        ("Destroy target creature.", "zone-change"),
        ("Scry 2.", "scry"),
        ("Investigate.", "investigate"),
        ("Tap target creature.", "tap"),
        ("Add {G}.", "add-mana"),
    ] {
        let effects = sequence_effects(text);
        assert_eq!(
            effects.iter().map(frame_of).collect::<Vec<_>>(),
            [frame],
            "{text}: {effects:?}"
        );
    }
}

#[test]
fn a_declared_subject_names_its_verb_never_a_missing_player() {
    // Near-miss of `target player mills`: an object subject with a verb no
    // subject frame reads is no longer the mill sink.
    let object = sequence_failure("Target creature can't activate abilities this turn.");
    assert_eq!(object.state, "clause.verb.unsupported", "{object:?}");
    assert!(
        object.expected.contains("a declared target object"),
        "{object:?}"
    );
    assert!(!object.expected.contains("exactly one target player"));

    let player = sequence_failure("Target opponent exiles the top two cards of their library.");
    assert_eq!(player.state, "clause.verb.unsupported", "{player:?}");
    assert!(player.expected.contains("lemma `exile`"), "{player:?}");
    assert!(
        player.expected.contains("a declared target player"),
        "{player:?}"
    );
}

#[test]
fn a_postposed_if_is_the_preposed_conditional() {
    let (preposed_status, preposed, preposed_states) = parse(
        "Instant",
        "If you control a legendary creature, draw a card.",
    );
    assert_eq!(preposed_status, "canonical", "{preposed_states:?}");
    let (postposed_status, postposed, postposed_states) = parse(
        "Instant",
        "Draw a card if you control a legendary creature.",
    );
    assert_eq!(postposed_status, "canonical", "{postposed_states:?}");
    assert_eq!(preposed, postposed);
    assert!(
        postposed
            .iter()
            .any(|rule| rule.to_string().contains("\"conditional\"")),
        "{postposed:?}"
    );
}

#[test]
fn a_postposed_if_gates_one_action_and_admits_no_otherwise() {
    // Two actions: the scope of a postposed `if` would be ambiguous.
    let (status, _, _) = parse(
        "Instant",
        "Draw a card and you gain 2 life if you control a legendary creature.",
    );
    assert_ne!(status, "canonical");
    let (status, _, _) = parse(
        "Instant",
        "Draw a card if you control a legendary creature. Otherwise, you gain 2 life.",
    );
    assert_ne!(status, "canonical");
}
