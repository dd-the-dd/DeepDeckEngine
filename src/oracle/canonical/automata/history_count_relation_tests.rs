//! Counted recorded actions read by their relation (`tokens you created
//! this turn`, `cards your opponents have drawn this turn`): the same
//! history pattern the condition language compares.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn pattern(text: &str) -> EventHistoryPattern {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("history-count-test", &tokens);
    match recognize_number_of_count(&mut cursor) {
        Ok(NumericExpression::EventHistoryCount(node)) => node.pattern,
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn ellyn_counts_the_tokens_its_controller_created_this_turn() {
    let created = pattern("the number of tokens you created this turn");
    assert_eq!(created.event, HistoryEvent::TokenCreated);
    assert_eq!(created.player, HistoryPlayer::Controller);
    assert_eq!(created.object, None);
    // The type line of the created token is the event's filter.
    let treasures = pattern("the number of Treasure tokens you created this turn");
    assert_eq!(
        treasures.object,
        Some(ObjectPredicate::Subtype("Treasure".to_string()))
    );
}

#[test]
fn opponents_perfect_and_controller_simple_past_share_the_pattern() {
    let drawn = pattern("the number of cards your opponents have drawn this turn");
    assert_eq!(drawn.event, HistoryEvent::CardDrawn);
    assert_eq!(drawn.player, HistoryPlayer::Opponent);
    // A previously unused combination: the simple past of another lemma.
    let drew = pattern("the number of cards you drew this turn");
    assert_eq!(drew.event, HistoryEvent::CardDrawn);
    assert_eq!(drew.player, HistoryPlayer::Controller);
}

#[test]
fn present_casts_results_and_nontoken_creations_are_not_this_history() {
    // `spells you cast` is the present cast relation, never a history.
    let tokens = recognize_entities("the number of spells you cast this turn");
    let mut cursor = OracleTokenCursor::new("history-count-test", &tokens);
    assert!(!matches!(
        recognize_number_of_count(&mut cursor),
        Ok(NumericExpression::EventHistoryCount(_))
    ));
    // `this way` is an action's result, not a this-turn history.
    let tokens = recognize_entities("for each card you discarded this way");
    let mut cursor = OracleTokenCursor::new("history-count-test", &tokens);
    assert!(!matches!(
        recognize_for_each_count(&mut cursor),
        Ok(NumericExpression::EventHistoryCount(_))
    ));
    let tokens = recognize_entities("the number of creatures you created this turn");
    let mut cursor = OracleTokenCursor::new("history-count-test", &tokens);
    let failure = recognize_number_of_count(&mut cursor).expect_err("a creation records tokens");
    assert_eq!(
        failure.state, "numeric.history-count.created-token",
        "{failure}"
    );
}
