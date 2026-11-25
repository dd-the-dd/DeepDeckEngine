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

    // A declared target player is a performer slot: a verb without a
    // lexicon lemma fails at the clause naming the word and the subject.
    let player = sequence_failure("Target opponent vanquishes a creature.");
    assert_eq!(player.state, "clause.verb.unsupported", "{player:?}");
    assert!(player.expected.contains("no lexicon lemma"), "{player:?}");
    assert!(
        player.expected.contains("a declared target player"),
        "{player:?}"
    );
    // A known lemma is read as the performer's own instruction: its
    // unsupported predicate is that reader's failure, past the subject,
    // never a missing player.
    let predicate = sequence_failure("Target opponent exiles the moon.");
    assert!(predicate.token_index >= 2, "{predicate:?}");
    assert!(
        !predicate.expected.contains("exactly one target player"),
        "{predicate:?}"
    );
}

#[test]
fn a_declared_target_player_performs_any_known_instruction() {
    // CR 608.2c, 115.1: the declared player performs the verb the clause
    // readers know; the sequence keeps one performer action.
    for text in [
        "Target player draws a card and loses 1 life.",
        "Target opponent exiles a card from their graveyard.",
        "Target opponent discards a card.",
    ] {
        let effects = sequence_effects(text);
        assert!(!effects.is_empty(), "{text}");
    }
    let effects = sequence_effects("Target opponent exiles a card from their graveyard.");
    assert!(
        matches!(
            effects.as_slice(),
            [LocalEffectNode::Performed(node)]
                if matches!(
                    node.performer,
                    super::super::PerformerNode::Player(
                        super::super::PlayerEffectSubject::DeclaredTarget { .. }
                    )
                )
        ),
        "{effects:?}"
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
fn a_postposed_if_gates_only_the_action_it_follows() {
    // After coordinated actions, a postposed `if` gates only the last one:
    // the card is drawn either way (CR 608.2c).
    let (status, rules, states) = parse(
        "Instant",
        "Draw a card and you gain 2 life if you control a legendary creature.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    let effects = &rules[0]["effects"];
    assert_eq!(effects[0]["kind"], "drawCards", "{effects:#}");
    assert_eq!(effects[1]["kind"], "conditional", "{effects:#}");
    assert_eq!(effects[1]["then"][0]["kind"], "gainLife", "{effects:#}");
    // When the gated action is the whole sentence, `Otherwise` is its
    // declined branch.
    let (status, rules, states) = parse(
        "Instant",
        "Draw a card if you control a legendary creature. Otherwise, you gain 2 life.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    assert_eq!(rules[0]["effects"][0]["else"][0]["kind"], "gainLife");
    // Near miss: after coordinated actions the scope of `Otherwise` would be
    // ambiguous, so it stays explicitly unsupported.
    let (status, _, states) = parse(
        "Instant",
        "Draw a card and you gain 2 life if you control a legendary creature. Otherwise, you lose 2 life.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "sequence.conditional.otherwise"),
        "{states:?}"
    );
}

#[test]
fn a_state_adjective_heads_an_object_set_subject() {
    // #18: `attacking creatures` is the shared collection, the subject of the
    // ordinary `get` predicate, like `creatures you control`.
    let effects = sequence_effects("Attacking creatures you control get +1/+0 until end of turn.");
    let [LocalEffectNode::ModifyStats(stats)] = effects.as_slice() else {
        panic!("one stat modification: {effects:?}");
    };
    assert!(
        matches!(
            stats.targets,
            super::super::PermanentOperandNode::Collection { .. }
        ),
        "{stats:?}"
    );
}
