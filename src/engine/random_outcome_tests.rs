//! The clash winner rule (CR 701.30d) in isolation: a strictly greater mana
//! value wins; a tie, or a player without a revealed card, never does.
use super::clash_winner;

fn revealed(values: &[(&str, Option<i32>)]) -> Vec<(String, Option<i32>)> {
    values
        .iter()
        .map(|(player, value)| ((*player).to_string(), *value))
        .collect()
}

#[test]
fn the_greater_mana_value_wins_a_clash() {
    assert_eq!(
        clash_winner(&revealed(&[("a", Some(4)), ("b", Some(2))])),
        Some("a".to_string())
    );
    assert_eq!(
        clash_winner(&revealed(&[("a", Some(0)), ("b", Some(5))])),
        Some("b".to_string())
    );
}

#[test]
fn a_tie_or_an_empty_library_has_no_winner_but_the_other_card_wins() {
    assert_eq!(
        clash_winner(&revealed(&[("a", Some(3)), ("b", Some(3))])),
        None
    );
    assert_eq!(clash_winner(&revealed(&[("a", None), ("b", None)])), None);
    // A player whose library was empty revealed nothing: any revealed card
    // has a greater mana value than no card at all.
    assert_eq!(
        clash_winner(&revealed(&[("a", None), ("b", Some(0))])),
        Some("b".to_string())
    );
}
