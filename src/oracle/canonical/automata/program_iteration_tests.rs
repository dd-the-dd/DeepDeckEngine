use super::super::super::OracleTokenCursor;
use super::super::NumericExpression;
use super::{
    IterationPopulation, RepeatedProcess, member_iteration_starts, player_iteration_population_at,
    recognize_player_iteration_introducer, recognize_repeat_process, repeat_process_starts,
};
use crate::oracle::syntax::recognize_entities;

#[test]
fn each_population_introducer_ends_at_its_comma() {
    for (text, population) in [
        (
            "for each opponent, draw a card",
            IterationPopulation::Opponents,
        ),
        ("For each player, draw a card", IterationPopulation::Players),
        (
            "for each other player, draw a card",
            IterationPopulation::OtherPlayers,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("program-iteration-unit", &tokens);
        assert_eq!(
            recognize_player_iteration_introducer(&mut cursor).expect(text),
            population,
            "{text}"
        );
        assert_eq!(cursor.peek(), Some("draw"), "{text}");
    }
    // `for each creature you control` is a countable, not a population, and
    // `for each opponent who ...` is not this introducer either.
    for text in [
        "for each creature you control, draw a card",
        "for each opponent who controls a creature, draw a card",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("program-iteration-unit", &tokens);
        assert!(player_iteration_population_at(&cursor).is_none(), "{text}");
    }
}

#[test]
fn a_partitive_over_a_bound_set_is_a_member_iteration() {
    for (text, starts) in [
        ("For each of them, create a token", true),
        ("For each of those creatures, create a token", true),
        ("For each creature you control, create a token", false),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("program-iteration-unit", &tokens);
        assert_eq!(member_iteration_starts(&cursor), starts, "{text}");
    }
}

fn repeat(text: &str) -> Result<RepeatedProcess, String> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("program-iteration-unit", &tokens);
    assert!(repeat_process_starts(&cursor), "{text}");
    recognize_repeat_process(&mut cursor)
        .map(|node| node.process)
        .map_err(|failure| failure.state.to_string())
}

#[test]
fn repetition_bounds_are_typed_counts() {
    assert_eq!(
        repeat("repeat this process.").unwrap(),
        RepeatedProcess::Preceding { additional: None }
    );
    assert_eq!(
        repeat("repeat this process once.").unwrap(),
        RepeatedProcess::Preceding {
            additional: Some(NumericExpression::from(1))
        }
    );
    assert_eq!(
        repeat("Repeat this process two more times.").unwrap(),
        RepeatedProcess::Preceding {
            additional: Some(NumericExpression::from(2))
        }
    );
    assert!(matches!(
        repeat("Repeat the following process X times.").unwrap(),
        RepeatedProcess::Following { .. }
    ));
}

#[test]
fn an_untyped_repetition_qualifier_fails_explicitly() {
    // `for an enchantment` (Invoke Despair) and `until ...` (Eureka) name
    // a different process or a different end; neither is approximated.
    for text in [
        "repeat this process for an enchantment and a planeswalker.",
        "Repeat this process until no one puts a card onto the battlefield.",
    ] {
        assert_eq!(
            repeat(text).unwrap_err(),
            "effect.repeat-process.complete",
            "{text}"
        );
    }
}
