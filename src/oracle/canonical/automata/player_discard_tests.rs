use crate::oracle::syntax::recognize_entities;

use super::super::super::super::super::OracleTokenCursor;
use super::super::super::super::{NumericAtom, NumericExpression, PlayerPredicate, TargetSelector};
use super::super::player_subject::{PlayerBinding, PlayerEffectSubject};
use super::super::selection_method::CollectionSelectionMethod;
use super::{DiscardQuantity, recognize_player_discard_effect};

fn parse(
    text: &str,
    antecedent: Option<PlayerBinding>,
) -> super::super::super::super::super::MachineResult<super::PlayerDiscardEffectNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.player-discard", &tokens);
    let effect = recognize_player_discard_effect(&mut cursor, PlayerBinding(11), antecedent)?;
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(effect)
}

#[test]
fn target_opponent_discards_word_quantity() {
    let effect =
        parse("Target opponent discards two cards.", None).expect("target-opponent discard parses");

    let PlayerEffectSubject::DeclaredTarget { target, binding } = effect.player else {
        panic!("expected a declared player target");
    };
    assert_eq!(binding, PlayerBinding(11));
    assert!(matches!(
        target.slots.as_slice(),
        [slot] if slot.selector == TargetSelector::Player(PlayerPredicate::Opponent)
    ));
    assert_eq!(
        effect.quantity,
        DiscardQuantity::Count(NumericExpression::Atom(NumericAtom::Integer(2)))
    );
    assert_eq!(effect.selection, CollectionSelectionMethod::Choice);
}

#[test]
fn controller_discards_a_single_card() {
    let effect = parse("You discard a card.", None).expect("controller discard parses");

    assert_eq!(effect.player, PlayerEffectSubject::Controller);
    assert_eq!(
        effect.quantity,
        DiscardQuantity::Count(NumericExpression::Atom(NumericAtom::Integer(1)))
    );
}

#[test]
fn bound_player_discards_variable_cards_at_random_without_becoming_a_target() {
    let antecedent = PlayerBinding(7);
    let effect = parse("That player discards X cards at random.", Some(antecedent))
        .expect("bound-player variable discard parses");

    assert_eq!(effect.player, PlayerEffectSubject::Bound(antecedent));
    assert_eq!(
        effect.quantity,
        DiscardQuantity::Count(NumericExpression::Atom(NumericAtom::VariableX))
    );
    assert_eq!(effect.selection, CollectionSelectionMethod::Random);
}

#[test]
fn controller_random_discard_is_a_collection_selection_modifier() {
    let effect =
        parse("You discard a card at random.", None).expect("postfix random discard parses");

    assert_eq!(effect.player, PlayerEffectSubject::Controller);
    assert_eq!(effect.selection, CollectionSelectionMethod::Random);
}

#[test]
fn numeric_digit_uses_the_shared_expression_grammar() {
    let effect =
        parse("Target player discards 3 cards.", None).expect("digit discard count parses");

    assert_eq!(
        effect.quantity,
        DiscardQuantity::Count(NumericExpression::Atom(NumericAtom::Integer(3)))
    );
}

#[test]
fn literal_one_requires_the_singular_card_noun() {
    let failure = parse("Target opponent discards one cards.", None)
        .expect_err("literal one with plural cards is a near miss");

    assert_eq!(failure.state, "counted-card.noun");
    assert_eq!(failure.actual.as_deref(), Some("cards"));
    assert!(failure.expected.contains("agreement"));
}

#[test]
fn that_player_without_a_binding_is_a_committed_failure() {
    let failure = parse("That player discards two cards.", None)
        .expect_err("an unbound player reference must not become a target");

    assert_eq!(failure.state, "player-subject.bound.resolve");
    assert_eq!(failure.actual.as_deref(), Some("That"));
    assert!(failure.expected.contains("prior player binding"));
}

#[test]
fn malformed_random_modifier_is_a_committed_near_miss() {
    let failure = parse("You discard a card at choice.", None)
        .expect_err("an `at` modifier must complete as `at random`");

    assert_eq!(failure.state, "selection-method.random.mode");
    assert_eq!(failure.actual.as_deref(), Some("choice"));
    assert!(failure.expected.contains("random"));
}

#[test]
fn the_whole_hand_is_the_acting_players_own_holder() {
    // One with Nothing / Bedlam Reveler (`discard your hand`), Dark Deal
    // (`discards all the cards in their hand`): one quantity determiner.
    for (text, player) in [
        ("You discard your hand.", PlayerEffectSubject::Controller),
        (
            "You discard all the cards in your hand.",
            PlayerEffectSubject::Controller,
        ),
        (
            "That player discards their hand.",
            PlayerEffectSubject::Bound(PlayerBinding(7)),
        ),
        (
            "That player discards all cards in their hand.",
            PlayerEffectSubject::Bound(PlayerBinding(7)),
        ),
    ] {
        let effect =
            parse(text, Some(PlayerBinding(7))).unwrap_or_else(|f| panic!("{text}: {f:?}"));
        assert_eq!(effect.player, player, "{text}");
        assert_eq!(effect.quantity, DiscardQuantity::WholeHand, "{text}");
    }
}

#[test]
fn another_players_hand_or_a_random_whole_hand_fails_typed() {
    for (text, state) in [
        (
            "You discard their hand.",
            "effect.player-discard.whole-hand.holder",
        ),
        (
            "That player discards all the cards in your hand.",
            "effect.player-discard.whole-hand.holder",
        ),
        (
            "You discard your hand at random.",
            "effect.player-discard.whole-hand-selection",
        ),
    ] {
        let failure = parse(text, Some(PlayerBinding(7))).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
}
