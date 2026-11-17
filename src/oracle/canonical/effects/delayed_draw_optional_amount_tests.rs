use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn optional_delayed_exact_amount_never_emits_an_unbound_second_draw() {
    for (text, whole_amount) in [
        (
            "you may draw two cards at the beginning of the next turn's upkeep",
            true,
        ),
        (
            "you may draw up to two cards at the beginning of the next turn's upkeep",
            false,
        ),
        (
            "you may draw a card at the beginning of the next turn's upkeep",
            false,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("optional-delayed-amount", &tokens);
        let node = recognize_delayed_draw(&mut cursor, None).unwrap();
        cursor.finish().unwrap();
        let schedule = lower_delayed_draw_schedule(&cursor, &node.delayed, None).unwrap();
        let effects = schedule["effects"].as_array().unwrap();
        if whole_amount {
            assert_eq!(effects.len(), 1);
            assert_eq!(effects[0]["kind"], "optionalEffects");
            assert_eq!(effects[0]["effects"][0]["count"], integer(2));
        } else {
            assert_eq!(effects.len(), 2);
            assert_eq!(effects[0]["kind"], "chooseNumber");
            assert_eq!(effects[0]["id"], DELAYED_DRAW_COUNT_ID);
            assert_eq!(effects[1]["kind"], "drawCards");
            assert_eq!(effects[1]["count"]["decisionId"], DELAYED_DRAW_COUNT_ID);
        }
    }
}
