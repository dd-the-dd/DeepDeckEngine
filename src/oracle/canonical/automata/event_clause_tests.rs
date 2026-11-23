//! Event patterns read their verbs from the lexicon. Generic modifiers
//! (counterpart, `alone`, `and isn't blocked`, `for the first time each
//! turn`) are typed; without a runtime matcher they fail as an engine gap,
//! distinct from grammar. A player-set subject whose reader does not take its
//! verb names the lemma and the subject kind (S5, S2).
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

fn parse(type_line: &str, text: &str) -> (String, Option<String>, Option<String>) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Event Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let failure = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "canonical")
        .and_then(|ability| ability.diagnostics.first());
    (
        parsed.status,
        failure.and_then(|diagnostic| diagnostic.state.clone()),
        failure.and_then(|diagnostic| diagnostic.expected.clone()),
    )
}

#[test]
fn combat_event_modifiers_compile_and_unsupported_ones_are_engine_gaps() {
    // Counterparts, `alone`, `and isn't blocked` and the attack ordinal have
    // a runtime matcher (docs/oracle-event-model.md).
    for text in [
        "Whenever this creature blocks a creature, draw a card.",
        "Whenever this creature becomes blocked by a creature, draw a card.",
        "Whenever this creature attacks or blocks, draw a card.",
        "Whenever this creature attacks and isn't blocked, draw a card.",
        "Whenever this creature attacks alone, draw a card.",
        "Whenever this creature attacks for the first time each turn, draw a card.",
        "Whenever this creature blocks a creature with flying, draw a card.",
    ] {
        assert_eq!(parse("Creature — Wall", text).0, "canonical", "{text}");
    }
    for text in [
        "Whenever this creature becomes blocked for the first time each turn, draw a card.",
        "Whenever this creature blocks a creature with power 2 or less, draw a card.",
    ] {
        assert_eq!(
            parse("Creature — Wall", text).1.as_deref(),
            Some("trigger.event.engine-gap"),
            "{text}"
        );
    }
}

#[test]
fn a_player_set_names_the_verb_it_does_not_read() {
    let (status, state, expected) = parse(
        "Sorcery",
        "Each player shuffles their hand and graveyard into their library.",
    );
    assert_ne!(status, "canonical");
    assert_eq!(state.as_deref(), Some("clause.verb.unsupported"));
    let expected = expected.unwrap_or_default();
    assert!(expected.contains("a player set"), "{expected}");
    // A population's permission is each player's own optional program
    // (the performer slot), no longer an unread verb.
    let (status, _, _) = parse(
        "Sorcery",
        "Each player may put a card from their hand onto the battlefield.",
    );
    assert_eq!(status, "canonical");
}
