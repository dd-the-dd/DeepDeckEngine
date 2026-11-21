//! Reader-level contract of player-state conditions: the subject fixes the
//! quantifier, the population and the possessive agreement; the measure and
//! its threshold are independent children.
use crate::oracle::syntax::recognize_entities;

use super::*;

fn read(text: &str) -> Result<PlayerStateConditionNode, (String, String)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-condition-test", &tokens);
    recognize_quantified_player_condition(&mut cursor)
        .map_err(|failure| (failure.state.to_string(), failure.expected))
}

fn subject(text: &str) -> ConditionPlayerSubject {
    read(text)
        .unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
        .subject
}

fn quantified(
    quantifier: PlayerQuantifier,
    population: PlayerPopulation,
) -> ConditionPlayerSubject {
    ConditionPlayerSubject::Quantified {
        quantifier,
        population,
    }
}

#[test]
fn the_determiner_and_noun_fix_quantifier_and_population() {
    use PlayerPopulation::{Opponents, Players};
    use PlayerQuantifier::{Every, None, Some};
    for (text, expected) in [
        (
            "an opponent has 10 or less life",
            quantified(Some, Opponents),
        ),
        ("a player has 10 or less life", quantified(Some, Players)),
        (
            "any player controls a black permanent",
            quantified(Some, Players),
        ),
        (
            "each opponent has 10 or less life",
            quantified(Every, Opponents),
        ),
        (
            "each player has no cards in hand",
            quantified(Every, Players),
        ),
        (
            "no opponent controls a creature",
            quantified(None, Opponents),
        ),
    ] {
        assert_eq!(subject(text), expected, "{text}");
    }
}

#[test]
fn measures_and_thresholds_compose_independently_of_the_subject() {
    let node = read("each opponent has at least three poison counters").expect("counter measure");
    assert_eq!(
        node.proposition,
        PlayerStateProposition {
            measure: PlayerStateMeasure::PlayerCounters(
                CounterName::from_parts(["poison"]).expect("counter name")
            ),
            threshold: CountThreshold::at_least(3),
        }
    );
    let node = read("a player has no cards in hand").expect("bare hand zone");
    assert_eq!(
        node.proposition.threshold,
        CountThreshold {
            operator: ThresholdOperator::AtMost,
            value: 0,
        }
    );
    assert!(matches!(
        node.proposition.measure,
        PlayerStateMeasure::CardsInZone {
            zone: PlayerCardZone::Hand,
            ..
        }
    ));
    let node = read("an opponent has drawn two or more cards this turn").expect("draw history");
    assert_eq!(
        node.proposition.measure,
        PlayerStateMeasure::CardsDrawnThisTurn
    );
}

#[test]
fn near_misses_fail_in_the_child_that_owns_the_mistake() {
    for (text, state) in [
        // `their` agrees with a third-person subject; `your` names another player.
        (
            "an opponent has eight or more cards in your graveyard",
            "condition.player.cards.possessive",
        ),
        // Counting spells through a filter has no evaluator.
        (
            "an opponent has cast two or more noncreature spells this turn",
            "condition.player.cast.filtered-count",
        ),
        // Counters on an object are not counters on the player.
        (
            "an opponent has three or more poison counters on it",
            "condition.player.counters.relation",
        ),
        // A life total is numeric.
        ("an opponent has no life", "condition.player.life.amount"),
        // A variable is not a fixed threshold.
        (
            "an opponent has X or more life",
            "condition.threshold.value",
        ),
    ] {
        let failure = read(text).expect_err(text);
        assert_eq!(failure.0, state, "{text}: {failure:?}");
    }
    let tokens = recognize_entities("a opponent controls a creature");
    let cursor = OracleTokenCursor::new("player-condition-test", &tokens);
    assert!(!quantified_player_condition_starts(&cursor));
    let tokens = recognize_entities("an opponent lost life this turn");
    let cursor = OracleTokenCursor::new("player-condition-test", &tokens);
    assert!(
        !quantified_player_condition_starts(&cursor),
        "other verbs keep their own readers"
    );
}
