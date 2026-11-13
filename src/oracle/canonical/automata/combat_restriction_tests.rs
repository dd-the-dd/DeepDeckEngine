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
