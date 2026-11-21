use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn temporary_allowance_is_an_ordinary_action_before_the_next_effect() {
    for text in [
        "you may play two additional lands this turn and draw a card.",
        "you may play two additional lands until end of turn, then draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("temporary-allowance-sequence", &tokens);
        let program = recognize_effect_program_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .unwrap();
        let EffectProgramNode::Sequence(sequence) = program else {
            panic!("may play grants future permission; it is not an optional execution choice")
        };
        assert!(matches!(
            &sequence.effects[..],
            [
                LocalEffectNode::TemporaryPlayerPermission(_),
                LocalEffectNode::DrawCards(_)
            ]
        ));
        let LocalEffectNode::TemporaryPlayerPermission(permission) = &sequence.effects[0] else {
            unreachable!()
        };
        assert_eq!(
            permission.modifier,
            PlayerModifierNode::AdditionalLandPlay {
                player: LocalPlayerReference::Controller,
                count: NumericExpression::Atom(NumericAtom::Integer(2)),
            }
        );
        assert_eq!(
            permission.duration,
            EffectDurationNode::UntilEndOfCurrentTurn
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn persistent_lifetime_remains_with_static_parent_and_rolls_back_temporary_child() {
    let tokens = recognize_entities("you may play an additional land on each of your turns.");
    let mut cursor = OracleTokenCursor::new("persistent-lifetime", &tokens);
    assert!(matches!(
        try_recognize_temporary_player_permission(&mut cursor),
        TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
    assert!(!temporary_land_permission_owns_head(&mut cursor));
    assert_eq!(cursor.position(), 0);
    recognize_static_additional_land_play(&mut cursor).unwrap();
    cursor.finish().unwrap();
}

#[test]
fn typed_temporary_lifetime_failure_is_committed_but_unrelated_play_is_not() {
    let tokens = recognize_entities("you may play an additional land this fortnight.");
    let mut cursor = OracleTokenCursor::new("temporary-lifetime-failure", &tokens);
    assert!(temporary_land_permission_owns_head(&mut cursor));
    assert_eq!(cursor.position(), 0);
    let TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure) =
        try_recognize_temporary_player_permission(&mut cursor)
    else {
        panic!("typed lifetime owns its missing turn")
    };
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");
    let tokens = recognize_entities("you may play cards from your graveyard.");
    let mut cursor = OracleTokenCursor::new("different-permission", &tokens);
    assert!(matches!(
        try_recognize_temporary_player_permission(&mut cursor),
        TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
}
