use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{GrantedKeyword, recognize_riot_keyword};

#[test]
fn riot_reminder_is_typed_as_counter_or_haste() {
    let tokens =
        recognize_entities("riot. (They enter with your choice of a +1/+1 counter or haste.)");
    let mut cursor = OracleTokenCursor::new("riot-keyword-test", &tokens);
    let riot = recognize_riot_keyword(&mut cursor).expect("riot parses");
    cursor.finish().expect("riot consumes its reminder");

    let reminder = riot.reminder.expect("typed reminder");
    assert_eq!(reminder.counter, "+1/+1");
    assert_eq!(reminder.alternative_keyword, GrantedKeyword::Haste);
}

#[test]
fn riot_without_reminder_remains_valid() {
    let tokens = recognize_entities("riot.");
    let mut cursor = OracleTokenCursor::new("riot-keyword-test", &tokens);
    let riot = recognize_riot_keyword(&mut cursor).expect("riot parses");
    cursor.finish().expect("keyword consumes all tokens");
    assert!(riot.reminder.is_none());
}

#[test]
fn malformed_reminder_fails_at_the_wrong_alternative_keyword() {
    let tokens =
        recognize_entities("riot. (They enter with your choice of a +1/+1 counter or flying.)");
    let mut cursor = OracleTokenCursor::new("riot-keyword-test", &tokens);
    let failure = recognize_riot_keyword(&mut cursor).expect_err("must fail");
    assert_eq!(failure.state, "keyword.riot.reminder.alternative");
}
