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
            permission.rule,
            TemporaryRuleNode::Allowance(PlayerModifierNode::AdditionalLandPlay {
                player: LocalPlayerReference::Controller,
                count: NumericExpression::Atom(NumericAtom::Integer(2)),
            })
        );
        assert_eq!(
            permission.duration,
            TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn)
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn persistent_lifetime_remains_with_static_parent_and_rolls_back_temporary_child() {
    let tokens = recognize_entities("you may play an additional land on each of your turns.");
    let mut cursor = OracleTokenCursor::new("persistent-lifetime", &tokens);
    assert!(matches!(
        try_recognize_temporary_player_permission(&mut cursor, None),
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
        try_recognize_temporary_player_permission(&mut cursor, None)
    else {
        panic!("typed lifetime owns its missing turn")
    };
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");
    let tokens = recognize_entities("you may play cards from your graveyard.");
    let mut cursor = OracleTokenCursor::new("different-permission", &tokens);
    assert!(matches!(
        try_recognize_temporary_player_permission(&mut cursor, None),
        TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
}

fn temporary_rule(text: &str) -> TemporaryPlayerPermissionParseAttempt {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("temporary-rule", &tokens);
    try_recognize_temporary_player_permission(&mut cursor, None)
}

#[test]
fn a_static_player_predicate_with_a_lifetime_is_a_temporary_rule() {
    for (text, duration) in [
        (
            "Your opponents can't cast spells this turn.",
            TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn),
        ),
        (
            "Players can't gain life this turn.",
            TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn),
        ),
        // Unseen combination: a qualified spell set, another population and
        // another lifetime.
        (
            "Each opponent can't cast noncreature spells until your next turn.",
            TemporaryRuleDuration::Effect(EffectDurationNode::UntilNextTurn {
                player: LocalPlayerReference::Controller,
            }),
        ),
    ] {
        let TemporaryPlayerPermissionParseAttempt::Parsed(node) = temporary_rule(text) else {
            panic!("{text}: a temporary rule");
        };
        assert!(
            matches!(node.rule, TemporaryRuleNode::Players { .. }),
            "{text}"
        );
        assert_eq!(node.duration, duration, "{text}");
    }
}

#[test]
fn game_and_hand_size_rules_take_the_same_lifetimes() {
    let TemporaryPlayerPermissionParseAttempt::Parsed(node) =
        temporary_rule("You have no maximum hand size for the rest of the game.")
    else {
        panic!("a hand-size rule");
    };
    assert_eq!(node.rule, TemporaryRuleNode::NoMaximumHandSize);
    assert_eq!(node.duration, TemporaryRuleDuration::RestOfGame);
    let TemporaryPlayerPermissionParseAttempt::Parsed(node) =
        temporary_rule("Damage can't be prevented this turn.")
    else {
        panic!("a damage rule");
    };
    assert_eq!(node.rule, TemporaryRuleNode::DamageCantBePrevented);
}

#[test]
fn the_next_spell_rule_reads_the_shared_spell_query() {
    for (text, modification) in [
        (
            "The next instant or sorcery spell you cast this turn can't be countered.",
            NextSpellModification::CantBeCountered,
        ),
        (
            "The next sorcery spell you cast this turn can be cast as though it had flash.",
            NextSpellModification::CastAsThoughFlash,
        ),
    ] {
        let TemporaryPlayerPermissionParseAttempt::Parsed(node) = temporary_rule(text) else {
            panic!("{text}: a next-spell rule");
        };
        assert!(
            matches!(node.rule, TemporaryRuleNode::NextSpell { modification: read, .. } if read == modification),
            "{text}"
        );
    }
    // A keyword the runtime does not grant to a next spell fails on its
    // rule, never as another frame's subject.
    let TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure) =
        temporary_rule("The next spell you cast this turn has convoke.")
    else {
        panic!("an unsupported next-spell rule is owned by this frame");
    };
    assert_eq!(failure.state, "temporary-rule.next-spell.rule");
}

#[test]
fn a_static_clause_without_lifetime_is_left_to_the_static_frame() {
    for text in [
        "Your opponents can't cast spells during your turn.",
        "Players can't gain life.",
        "X can't be 0.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("static-near-miss", &tokens);
        assert!(
            matches!(
                try_recognize_temporary_player_permission(&mut cursor, None),
                TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_)
                    | TemporaryPlayerPermissionParseAttempt::NotClaimed
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
}

/// A preposed lifetime is the lifetime of each rule of its sentence; the
/// same rule may not state a second one, and without either lifetime the
/// rule body is not this frame's (it rolls back).
#[test]
fn a_supplied_lifetime_is_exclusive_and_required() {
    let supplied = EffectDurationNode::UntilNextTurn {
        player: LocalPlayerReference::Controller,
    };
    for text in [
        "artifacts your opponents control enter tapped.",
        "the \"legend rule\" doesn't apply to permanents you control.",
        "players can't draw cards.",
        "you may play lands and cast spells from your graveyard.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("supplied-lifetime", &tokens);
        let TemporaryPlayerPermissionParseAttempt::Parsed(node) =
            try_recognize_temporary_player_permission(&mut cursor, Some(&supplied))
        else {
            panic!("{text}: the supplied lifetime closes the rule");
        };
        assert_eq!(
            node.duration,
            TemporaryRuleDuration::Effect(supplied.clone()),
            "{text}"
        );
        let mut cursor = OracleTokenCursor::new("missing-lifetime", &tokens);
        assert!(
            matches!(
                try_recognize_temporary_player_permission(&mut cursor, None),
                TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_)
                    | TemporaryPlayerPermissionParseAttempt::NotClaimed
            ),
            "{text}: a rule body without a lifetime is not a temporary rule"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
    let tokens = recognize_entities("players can't draw cards this turn.");
    let mut cursor = OracleTokenCursor::new("second-lifetime", &tokens);
    assert!(matches!(
        try_recognize_temporary_player_permission(&mut cursor, Some(&supplied)),
        TemporaryPlayerPermissionParseAttempt::FailedCommitted(_)
    ));
}
