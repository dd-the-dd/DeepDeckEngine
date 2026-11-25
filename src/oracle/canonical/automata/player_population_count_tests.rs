//! Counted player populations (`for each opponent who lost life this
//! turn`, `the number of opponents who control an artifact`): the shared
//! player-set restriction read after a counting introducer.
use super::*;
use crate::oracle::canonical::effects::{
    NumericQueryLoweringSupport, lower_numeric_expression_with_query_support,
};
use crate::oracle::syntax::recognize_entities;
use serde_json::json;

fn read(text: &str, number_of: bool) -> MachineResult<NumericExpression> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-count-test", &tokens);
    if number_of {
        recognize_number_of_count(&mut cursor)
    } else {
        recognize_for_each_count(&mut cursor)
    }
}

fn lowered(text: &str, number_of: bool) -> serde_json::Value {
    let count = read(text, number_of).unwrap_or_else(|failure| panic!("{text}: {failure}"));
    lower_numeric_expression_with_query_support(&count, NumericQueryLoweringSupport::Unavailable)
        .unwrap_or_else(|| panic!("{text}: no lowering"))
}

#[test]
fn kellan_counts_opponents_who_control_an_artifact() {
    let count = lowered("the number of opponents who control an artifact", true);
    assert_eq!(count["kind"], "countPlayers");
    assert_eq!(count["players"]["kind"], "filterPlayers");
    assert_eq!(count["players"]["players"]["kind"], "opponentsOf");
    assert_eq!(count["players"]["where"]["kind"], "compare");
}

#[test]
fn member_histories_and_possessions_share_the_counted_population() {
    let lost = lowered("for each opponent who lost life this turn", false);
    assert_eq!(
        lost["players"]["where"],
        json!({
            "kind": "compare",
            "operator": ">=",
            "left": { "kind": "lifeLostThisTurn", "player": { "kind": "quantifiedPlayer" } },
            "right": { "kind": "integer", "value": 1 },
        })
    );
    let combat = lowered(
        "for each player who was dealt combat damage this turn",
        false,
    );
    assert_eq!(combat["players"]["players"]["kind"], "eachPlayer");
    assert_eq!(
        combat["players"]["where"]["left"]["detail"],
        json!({ "combat": true })
    );
    // A previously unused combination: a plural member history.
    let plural = lowered(
        "the number of opponents who were dealt damage this turn",
        true,
    );
    assert_eq!(plural["players"]["where"]["left"]["event"], "damageDealt");
    let poison = lowered(
        "for each opponent with three or more poison counters",
        false,
    );
    assert_eq!(poison["players"]["where"]["kind"], "compare");
}

#[test]
fn unread_relatives_and_disagreeing_verbs_fail_explicitly() {
    let unread = read("for each opponent who searches a library this way", false)
        .expect_err("an untyped member action is not counted");
    assert_eq!(unread.state, "player-set.relative.who.verb", "{unread}");
    let disagreeing = read(
        "the number of opponents who was dealt damage this turn",
        true,
    )
    .expect_err("a plural member takes `were`");
    assert_eq!(
        disagreeing.state, "player-set.relative.who.dealt-damage",
        "{disagreeing}"
    );
}
