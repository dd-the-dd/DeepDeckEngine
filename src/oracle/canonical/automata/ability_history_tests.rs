//! Reader-level contract of the resolving ability's own record: the
//! ordinal (or ordinal range, or threshold) fixes one comparison of one
//! count; the record names resolutions or activations.
use crate::oracle::syntax::recognize_entities;

use super::super::ConditionScope;
use super::*;

fn comparison(atom: ConditionAtom) -> (AbilityHistoryMeasure, NumericComparisonOperator, i64) {
    let ConditionAtom::Compare(ConditionComparison {
        left: ConditionNumber::Expression(NumericExpression::Quantity(quantity)),
        operator,
        right: ConditionNumber::Integer(value),
        ..
    }) = atom
    else {
        panic!("an ability-history comparison");
    };
    let NumericQuantity::AbilityHistory(measure) = *quantity else {
        panic!("an ability-history quantity");
    };
    (measure, operator, value)
}

fn read_at(text: &str, start: usize) -> Result<ConditionAtom, String> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("ability-history-test", &tokens);
    cursor
        .advance_to(start)
        .expect("a token position of the text");
    if !ability_history_condition_starts(&cursor) {
        return Err("not claimed".to_string());
    }
    recognize_ability_history_condition(&mut cursor).map_err(|failure| failure.state.to_string())
}

fn read(text: &str) -> Result<ConditionAtom, String> {
    read_at(text, 0)
}

#[test]
fn an_ordinal_is_one_equality_and_a_leading_range_an_upper_bound() {
    use AbilityHistoryMeasure::ResolutionsThisTurn;
    use NumericComparisonOperator::{Equal, LessThanOrEqual};
    for (text, expected) in [
        (
            "this is the third time this ability has resolved this turn",
            (ResolutionsThisTurn, Equal, 3),
        ),
        (
            "this is the first time this ability has resolved this turn",
            (ResolutionsThisTurn, Equal, 1),
        ),
        (
            "this is the first or second time this ability has resolved this turn",
            (ResolutionsThisTurn, LessThanOrEqual, 2),
        ),
    ] {
        let atom = read(text).unwrap_or_else(|state| panic!("{text}: {state}"));
        assert_eq!(comparison(atom), expected, "{text}");
    }
}

#[test]
fn an_ordinal_list_with_a_gap_has_no_single_comparison() {
    assert_eq!(
        read("this is the first or third time this ability has resolved this turn"),
        Err("condition.ability-history.ordinal-list".to_string())
    );
}

#[test]
fn a_threshold_counts_activations_or_resolutions() {
    let atom = read("this ability has been activated four or more times this turn")
        .expect("an activation threshold");
    assert_eq!(
        comparison(atom),
        (
            AbilityHistoryMeasure::ActivationsThisTurn,
            NumericComparisonOperator::GreaterThanOrEqual,
            4
        )
    );
    let atom = read("this ability has resolved three or more times this turn")
        .expect("a resolution threshold");
    assert_eq!(
        comparison(atom).0,
        AbilityHistoryMeasure::ResolutionsThisTurn
    );
}

#[test]
fn an_elided_ordinal_needs_an_earlier_sentence_stating_the_record() {
    let stated = "you gain 1 life if this is the first time this ability has resolved this turn. If it's the second time, draw a card.";
    let tokens = recognize_entities(stated);
    let start = tokens
        .iter()
        .position(|token| {
            token
                .raw
                .replace('\u{2019}', "'")
                .eq_ignore_ascii_case("it's")
        })
        .expect("the elided ordinal");
    let atom = read_at(stated, start).expect("the restored record");
    assert_eq!(
        comparison(atom),
        (
            AbilityHistoryMeasure::ResolutionsThisTurn,
            NumericComparisonOperator::Equal,
            2
        )
    );
    // Without a stated record, `it's the second time` is not this reader's.
    assert_eq!(read("it's the second time"), Err("not claimed".to_string()));
}

#[test]
fn only_a_program_position_reads_the_record() {
    let atom = read("this is the second time this ability has resolved this turn")
        .expect("a resolution ordinal");
    assert!(atom.reads_ability_history());
    assert!(ConditionScope::Program.allows_atom(&atom));
    for scope in [
        ConditionScope::Static,
        ConditionScope::TriggerGuard,
        ConditionScope::Unless,
    ] {
        assert!(!scope.allows_atom(&atom), "{scope:?}");
    }
}
