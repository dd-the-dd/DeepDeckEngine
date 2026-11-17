use super::*;

// Real Oracle text (Spirit Mascot) and its singular sibling: the verb is the
// event, the quantifier is the subject's multiplicity (CR 603.2c).
const ONE_OR_MORE: &str =
    "Whenever one or more cards leave your graveyard, put a +1/+1 counter on this creature.";
const EACH_CARD: &str =
    "Whenever a card leaves your graveyard, put a +1/+1 counter on this creature.";

fn watcher(text: &str) -> CardInstance {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Graveyard watcher".to_string(),
        type_line: "Creature — Spirit".to_string(),
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    let mut definition = test_definition("graveyard-watcher", "Creature - Spirit");
    definition.rules = vec![rule];
    test_instance("graveyard-watcher", definition, "player-0")
}

/// Two independent effects, each moving three cards out of the graveyard.
fn two_effects_of_three_cards(text: &str) -> usize {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![watcher(text)];
    for effect in 0..2 {
        let ids = (0..3)
            .map(|card| format!("moved-{effect}-{card}"))
            .collect::<Vec<_>>();
        engine.state.players[0].exile.extend(
            ids.iter()
                .map(|id| test_instance(id, test_definition(id, "Instant"), "player-0")),
        );
        engine.enqueue_cards_left_graveyard_triggers("player-0", &ids);
    }
    engine.state.stack.len()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn one_or_more_triggers_once_per_event_and_a_card_once_per_card() {
    assert_eq!(
        two_effects_of_three_cards(ONE_OR_MORE),
        2,
        "one trigger per event"
    );
    assert_eq!(
        two_effects_of_three_cards(EACH_CARD),
        6,
        "one trigger per card"
    );
}
