use super::super::TemporaryKeywordGrantObjectsNode;
use super::{TemporaryCombatRestrictionParseAttempt, try_recognize_temporary_combat_restriction};
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> TemporaryCombatRestrictionParseAttempt {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("temporary-combat-restriction-test", &tokens);
    try_recognize_temporary_combat_restriction(&mut cursor)
}

#[test]
fn recognizes_three_independent_combat_restrictions() {
    for text in [
        "Target creature can't attack this turn",
        "Target creature can't block this turn",
        "Target creature can't be blocked this turn",
    ] {
        let TemporaryCombatRestrictionParseAttempt::Parsed(node) = parse(text) else {
            panic!("expected temporary combat restriction: {text}");
        };
        assert!(matches!(
            node.objects,
            TemporaryKeywordGrantObjectsNode::Targets(_)
        ));
        assert_eq!(node.keywords.len(), 1);
    }
}

#[test]
fn retains_shared_qualified_target_and_next_turn_duration() {
    for text in [
        "Target creature with power 2 or less can't be blocked this turn",
        "Target creature can't attack until your next turn",
    ] {
        assert!(matches!(
            parse(text),
            TemporaryCombatRestrictionParseAttempt::Parsed(_)
        ));
    }
}

#[test]
fn rolls_back_before_prohibition_and_commits_after_it() {
    assert!(matches!(
        parse("Target creature gets +1/+1 until end of turn"),
        TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_)
    ));
    assert!(matches!(
        parse("Target creature can't be restored this turn"),
        TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_)
    ));
    assert!(matches!(
        parse("Target creature can't activate abilities this turn"),
        TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_)
    ));
    assert!(matches!(
        parse("Target creature can't be blocked this combat"),
        TemporaryCombatRestrictionParseAttempt::FailedCommitted(_)
    ));
}

/// `can attack DURATION as though it didn't have defender` (CR 702.3b): the
/// clause's own duration, the defender exception marker; near misses fail
/// after the combat action word.
#[test]
fn a_temporary_defender_exception_is_one_marker_with_its_own_duration() {
    let TemporaryCombatRestrictionParseAttempt::Parsed(node) = parse(
        "Target creature with defender can attack this turn as though it didn't have defender",
    ) else {
        panic!("expected the defender exception")
    };
    assert_eq!(
        node.keywords,
        vec![super::super::GrantedKeyword::AttackAsThoughNoDefender]
    );
    assert_eq!(
        node.duration,
        super::super::EffectDurationNode::UntilEndOfCurrentTurn
    );
    // `as though it had defender` is no exception: not even claimed.
    assert!(matches!(
        parse("Target creature can attack this turn as though it had defender"),
        TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_)
    ));
}
