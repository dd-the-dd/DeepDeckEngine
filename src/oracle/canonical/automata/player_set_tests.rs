use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn each_opponent_reuses_the_existing_opponents_population_and_stops_before_the_action() {
    for (text, expected) in [
        (
            "Each opponent loses 2 life.",
            PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
        (
            "Each player gains 1 life.",
            PlayerSetReferenceNode::EachPlayer,
        ),
        (
            "Your opponents can't cast spells.",
            PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("player-population", &tokens);
        assert_eq!(
            recognize_player_set_reference(&mut cursor).unwrap(),
            expected
        );
        assert_eq!(cursor.position(), 2);
        assert!(cursor.peek().is_some());
    }
}

#[test]
fn historical_opponents_trace_is_preserved() {
    let tokens = recognize_entities("Your opponents");
    let mut cursor = OracleTokenCursor::new("player-population", &tokens);
    recognize_player_set_reference(&mut cursor).unwrap();
    let states = cursor
        .transitions()
        .iter()
        .filter_map(|transition| {
            transition
                .state
                .starts_with("player-set.")
                .then_some(transition.state)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        vec![
            "player-set.possessor",
            "player-set.role",
            "player-set.complete"
        ]
    );
}

#[test]
fn unknown_and_plural_roles_remain_structured_failures_without_a_fallback_population() {
    for text in ["Each opponents", "Each creature", "Your enemy"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("player-population", &tokens);
        let failure = recognize_player_set_reference(&mut cursor).unwrap_err();
        assert_eq!(failure.state, "player-set.role");
    }
}
