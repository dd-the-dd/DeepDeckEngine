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
        // The attacker of a block record counts the combats it became
        // blocked in (S5 action-record ordinal).
        "Whenever this creature becomes blocked for the first time each turn, draw a card.",
    ] {
        assert_eq!(parse("Creature — Wall", text).0, "canonical", "{text}");
    }
    for text in [
        "Whenever this creature becomes blocked by a creature for the first time each turn, draw a card.",
        "Whenever this creature blocks a creature with power 2 or less, draw a card.",
    ] {
        assert_eq!(
            parse("Creature — Wall", text).1.as_deref(),
            Some("engine.gap.event-matcher"),
            "{text}"
        );
    }
}

#[test]
fn a_player_set_names_the_verb_it_does_not_read() {
    // The clause layer reads a player set's zone shuffle (Timetwister);
    // `exert` is a lexicon lemma no player-set reader takes.
    let (status, _, _) = parse(
        "Sorcery",
        "Each player shuffles their hand and graveyard into their library.",
    );
    assert_eq!(status, "canonical");
    let (status, state, expected) = parse("Sorcery", "Each player exerts a creature they control.");
    assert_ne!(status, "canonical");
    assert_eq!(state.as_deref(), Some("clause.verb.unsupported"));
    let expected = expected.unwrap_or_default();
    assert!(expected.contains("a player set"), "{expected}");
    // A lexicon lemma is named by its base form, after `may` too: the
    // population's permission reads the performer's instruction, whose
    // unread verb stays the clause's diagnostic.
    let (_, state, expected) = parse("Sorcery", "Each player may exert a creature they control.");
    assert_eq!(state.as_deref(), Some("clause.verb.unsupported"));
    assert!(expected.unwrap_or_default().contains("lemma `exert`"));
}

#[test]
fn the_shared_modifier_tail_composes_with_every_combat_subject() {
    // Previously unused combinations: the turn owner on a source block, and
    // `alone` / `and isn't blocked` / the turn ordinal on a quantified
    // attacker. No new production is needed per combination.
    for text in [
        "Whenever this creature blocks during an opponent's turn, draw a card.",
        "Whenever this creature attacks during your turn, draw a card.",
        "Whenever a creature you control attacks alone, draw a card.",
        "Whenever a creature you control attacks and isn't blocked, draw a card.",
        "Whenever another creature you control attacks for the first time each turn, draw a card.",
        "Whenever a creature an opponent controls attacks during your turn, draw a card.",
    ] {
        assert_eq!(parse("Creature — Wall", text).0, "canonical", "{text}");
    }
}

#[test]
fn a_turn_owner_without_a_pattern_matcher_is_an_event_time_condition() {
    // Whose turn it is is a state when the event occurs (CR 500.1, 603.2):
    // the occurrence keeps its own matcher and the ability its
    // `eventCondition`, never an engine gap and never dropped.
    for text in [
        "Whenever a creature you control dies during your turn, draw a card.",
        "Whenever this creature enters during your turn, draw a card.",
        "Whenever you gain life during your turn, draw a card.",
        "Whenever an opponent draws a card during your turn, draw a card.",
    ] {
        assert_eq!(parse("Creature — Wall", text).0, "canonical", "{text}");
    }
}

#[test]
fn a_recognized_modifier_without_its_matcher_is_an_engine_gap() {
    for text in [
        "Whenever a creature you control blocks alone, draw a card.",
        "Whenever a creature you control attacks and isn't blocked for the first time each turn, draw a card.",
        "Whenever an opponent gains life for the first time each turn, draw a card.",
    ] {
        assert_eq!(
            parse("Creature — Wall", text).1.as_deref(),
            Some("engine.gap.event-matcher"),
            "{text}"
        );
    }
}
