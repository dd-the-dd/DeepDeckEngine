//! The player subject is claimed only before a predicate this frame owns,
//! and every coordinated prohibited action is read or rejected, never dropped.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn read(text: &str) -> MachineResult<(StaticPlayerSubjectReading, StaticPlayerPredicateNode)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("static-player-test", &tokens);
    let reading = recognize_static_player_subject(&mut cursor)?;
    let predicate = recognize_static_player_predicate(&mut cursor, reading)?;
    Ok((reading, predicate))
}

#[test]
fn player_subject_claims_only_before_an_owned_predicate() {
    for (text, claimed) in [
        ("You have hexproof.", true),
        ("Each of your opponents has shroud.", true),
        ("Enchanted creature's controller can't gain life.", true),
        ("You control enchanted creature.", false),
        ("You may cast spells from your graveyard.", false),
        ("Each player sacrifices a creature.", false),
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("static-player-test", &tokens);
        let claims = static_player_subject_width(&cursor).is_some_and(|(_, width)| {
            let mut ahead = cursor.clone();
            ahead.advance_to(width).is_ok() && static_player_predicate_starts(&ahead)
        });
        assert_eq!(claims, claimed, "{text}");
    }
}

#[test]
fn coordinated_prohibited_actions_are_each_read() {
    let (_, predicate) =
        read("Your opponents can't cast spells or activate abilities of artifacts.").unwrap();
    let StaticPlayerPredicateNode::Prohibited(actions) = predicate else {
        panic!("a prohibition");
    };
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0],
        ProhibitedPlayerActionNode::CastSpells { window: None, .. }
    ));
    assert!(matches!(
        actions[1],
        ProhibitedPlayerActionNode::ActivateAbilities { sources: Some(_) }
    ));
    let failure = read("Players can't gain life or pay life.").unwrap_err();
    assert_eq!(failure.state, "static.player.prohibition.action");
}

#[test]
fn possessive_subject_reads_signed_hand_size_changes_and_verbs_agree() {
    for (text, change) in [
        (
            "Your maximum hand size is increased by one.",
            HandSizeChangeNode::Adjust(1),
        ),
        (
            "Each opponent's maximum hand size is reduced by seven.",
            HandSizeChangeNode::Adjust(-7),
        ),
        (
            "Your maximum hand size is seven.",
            HandSizeChangeNode::Set(7),
        ),
    ] {
        let (reading, predicate) = read(text).unwrap();
        assert!(reading.possessive, "{text}");
        assert_eq!(
            predicate,
            StaticPlayerPredicateNode::MaximumHandSize(change)
        );
    }
    assert!(read("Each opponent have hexproof.").is_err());
    assert!(read("You has hexproof.").is_err());
}
