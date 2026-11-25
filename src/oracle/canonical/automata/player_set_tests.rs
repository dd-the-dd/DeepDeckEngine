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

fn restriction(
    text: &str,
    players: PlayerSetReferenceNode,
) -> (MachineResult<PlayerPopulationQueryNode>, Option<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-population", &tokens);
    let result = recognize_player_population_restriction(
        &mut cursor,
        players,
        false,
        &PlayerRelativeContext::default(),
    );
    (result, cursor.peek().map(str::to_string))
}

#[test]
fn relative_predicates_reuse_the_player_state_propositions_and_stop_before_the_verb() {
    for (text, verb, negated) in [
        (
            "who controls a multicolored creature draws a card.",
            "draws",
            false,
        ),
        (
            "who controls six or more lands chooses five lands.",
            "chooses",
            false,
        ),
        ("who doesn't control an Elf loses 1 life.", "loses", true),
        ("with exactly 13 life loses the game.", "loses", false),
        (
            "who has seven or more cards in their hand discards a card.",
            "discards",
            false,
        ),
    ] {
        let (result, next) = restriction(text, PlayerSetReferenceNode::EachPlayer);
        let query = result.unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert!(matches!(
            query.predicate,
            Some(PlayerRelativePredicate::State { negated: written, .. }) if written == negated
        ));
        assert_eq!(next.as_deref(), Some(verb), "{text}");
    }
}

#[test]
fn superlatives_compare_every_player_and_refuse_a_sub_population() {
    let (result, next) = restriction(
        "who controls the most lands sacrifices two lands.",
        PlayerSetReferenceNode::EachPlayer,
    );
    assert!(matches!(
        result.unwrap().predicate,
        Some(PlayerRelativePredicate::Superlative {
            extremum: PlayerExtremum::Greatest,
            ..
        })
    ));
    assert_eq!(next.as_deref(), Some("sacrifices"));
    let (result, _) = restriction(
        "with the lowest life total gains control of it.",
        PlayerSetReferenceNode::EachPlayer,
    );
    assert!(matches!(
        result.unwrap().predicate,
        Some(PlayerRelativePredicate::Superlative {
            measure: super::super::PlayerStateMeasure::Life,
            extremum: PlayerExtremum::Least,
        })
    ));
    let (result, _) = restriction(
        "who controls the most lands sacrifices a land.",
        PlayerSetReferenceNode::OpponentsOfAbilityController,
    );
    assert_eq!(result.unwrap_err().state, "player-set.relative.superlative");
}

#[test]
fn untyped_relative_clauses_fail_with_their_own_states() {
    for (text, state) in [
        (
            "whose life total is 10 or less draws a card.",
            "player-set.relative.whose",
        ),
        (
            "who attacked this turn draws a card.",
            "player-set.relative.who.verb",
        ),
        ("who can't discards a card.", "player-set.relative.who.verb"),
        (
            "other than target player creates a token.",
            "player-set.relative.exclusion.player",
        ),
    ] {
        let (result, _) = restriction(text, PlayerSetReferenceNode::EachPlayer);
        assert_eq!(result.unwrap_err().state, state, "{text}");
    }
}
