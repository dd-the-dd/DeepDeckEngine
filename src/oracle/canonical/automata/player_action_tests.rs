use super::super::super::OracleTokenCursor;
use super::super::{
    EffectRecognitionContext, LocalEffectNode, PlayerBinding, PlayerEffectSubject,
    recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::recognize_entities;

fn parse(
    text: &str,
) -> Result<super::super::EffectSequenceNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-action-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
}

#[test]
fn declaration_is_allocated_once_and_omitted_subject_is_bound() {
    let sequence = parse("Target player draws two cards and loses 2 life.").unwrap();
    let [
        LocalEffectNode::PlayerAction(first),
        LocalEffectNode::PlayerAction(second),
    ] = sequence.effects.as_slice()
    else {
        panic!("two independent actions")
    };
    assert!(matches!(
        &first.player,
        PlayerEffectSubject::DeclaredTarget {
            binding: PlayerBinding(0),
            ..
        }
    ));
    assert_eq!(second.player, PlayerEffectSubject::Bound(PlayerBinding(0)));
    assert!(matches!(first.action, super::SingularPlayerAction::Draw(_)));
    assert!(matches!(
        second.action,
        super::SingularPlayerAction::Life(_)
    ));
}

#[test]
fn explicit_actor_replaces_grammatical_subject_and_period_ends_ellipsis() {
    let sequence =
        parse("Target player draws a card and you lose 1 life and draw a card.").unwrap();
    for action in &sequence.effects[1..] {
        let LocalEffectNode::PlayerAction(action) = action else {
            panic!("shared player action")
        };
        assert_eq!(action.player, PlayerEffectSubject::Controller);
    }
    assert!(parse("Target player draws a card. Loses 1 life.").is_err());
    assert!(parse("Target player draws a card and lose 1 life.").is_err());
    assert!(parse("Target player draws a card. That player loses 1 life.").is_ok());
}

#[test]
fn unrelated_and_nested_programs_do_not_borrow_the_current_subject() {
    for text in [
        "Target player draws a card and scry 1 and loses 1 life.",
        "Target player draws two cards and discard a card.",
        "Target player draws a card. You may draw a card and loses 1 life.",
        "That player draws a card.",
        "Draws a card.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

#[test]
fn player_declarations_and_permanent_subjects_remain_distinct() {
    let sequence =
        parse("Target player draws a card and target player draws two cards and loses 2 life.")
            .unwrap();
    let [
        LocalEffectNode::PlayerAction(first),
        LocalEffectNode::PlayerAction(second),
        LocalEffectNode::PlayerAction(last),
    ] = sequence.effects.as_slice()
    else {
        panic!("three actions")
    };
    assert!(matches!(
        &first.player,
        PlayerEffectSubject::DeclaredTarget {
            binding: PlayerBinding(0),
            ..
        }
    ));
    assert!(matches!(
        &second.player,
        PlayerEffectSubject::DeclaredTarget {
            binding: PlayerBinding(1),
            ..
        }
    ));
    assert_eq!(last.player, PlayerEffectSubject::Bound(PlayerBinding(1)));
    assert!(parse("Target player draws a card and it gets +1/+1 until end of turn.").is_err());
}

#[test]
fn binding_errors_commit_only_after_the_child_owns_an_action() {
    let context = EffectRecognitionContext::default();
    for text in [
        "That player may search their library for a card.",
        "That player may draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("player-action-transaction-unit", &tokens);
        let result =
            super::try_recognize_player_action(&mut cursor, PlayerBinding(7), None, None, &context)
                .unwrap();
        assert!(result.is_none(), "{text}");
        assert_eq!(cursor.position(), 0, "{text}");
    }

    let tokens = recognize_entities("That player draws a card.");
    let mut cursor = OracleTokenCursor::new("player-action-transaction-unit", &tokens);
    let error =
        super::try_recognize_player_action(&mut cursor, PlayerBinding(7), None, None, &context)
            .unwrap_err();
    assert_eq!(error.state, "player-subject.bound.resolve");

    let mut cursor = OracleTokenCursor::new("player-action-transaction-unit", &tokens);
    let action = super::try_recognize_player_action(
        &mut cursor,
        PlayerBinding(7),
        Some(PlayerBinding(3)),
        None,
        &context,
    )
    .unwrap()
    .unwrap();
    assert_eq!(action.player, PlayerEffectSubject::Bound(PlayerBinding(3)));

    let tokens = recognize_entities("discard a card.");
    let mut cursor = OracleTokenCursor::new("player-action-transaction-unit", &tokens);
    let inherited = PlayerEffectSubject::Bound(PlayerBinding(3));
    let error = super::try_recognize_player_action(
        &mut cursor,
        PlayerBinding(7),
        None,
        Some(&inherited),
        &context,
    )
    .unwrap_err();
    assert_eq!(error.state, "player-subject.action.scope");
}

#[test]
fn mill_is_a_player_verb_composed_with_the_shared_subject_and_count() {
    // Sage's Row Denizen effect, plus a new combination with an inherited
    // subject: declaration, binding and verb are independent concepts.
    for (text, actions) in [
        ("Target player mills two cards.", 1),
        ("Target player mills 2 cards.", 1),
        ("Target player mills a card.", 1),
        ("Target player draws a card and mills three cards.", 2),
    ] {
        let sequence = parse(text).expect(text);
        assert_eq!(sequence.effects.len(), actions, "{text}");
        let LocalEffectNode::PlayerAction(last) = sequence.effects.last().unwrap() else {
            panic!("shared player action: {text}")
        };
        assert!(
            matches!(last.action, super::SingularPlayerAction::Mill(_)),
            "{text}"
        );
    }
    // Near misses: verb agreement and counted-noun agreement are enforced.
    for text in [
        "Target player mill two cards.",
        "Target player mills two card.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

#[test]
fn imperative_mill_is_performed_by_the_controller() {
    // Scarblade Scout's effect and a sibling followed by another action.
    for (text, actions) in [
        ("Mill two cards.", 1),
        ("Mill a card, then draw a card.", 2),
    ] {
        let sequence = parse(text).expect(text);
        assert_eq!(sequence.effects.len(), actions, "{text}");
        let LocalEffectNode::PlayerAction(first) = &sequence.effects[0] else {
            panic!("shared player action: {text}")
        };
        assert_eq!(first.player, PlayerEffectSubject::Controller, "{text}");
        assert!(matches!(first.action, super::SingularPlayerAction::Mill(_)));
    }
    // Near miss: the imperative takes the base form only.
    assert!(parse("Mills two cards.").is_err());
}
