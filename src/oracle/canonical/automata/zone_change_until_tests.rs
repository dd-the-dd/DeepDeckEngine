use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::super::ZoneChangeUntilNode;
use super::{recognize_zone_change_until, zone_change_until_starts};

#[test]
fn source_departure_ends_the_zone_change_for_any_source_noun() {
    for text in [
        "until this enchantment leaves the battlefield",
        "until this Aura leaves the battlefield",
        "until this creature leaves the battlefield",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("until-test", &tokens);
        assert!(zone_change_until_starts(&cursor), "{text}");
        assert_eq!(
            recognize_zone_change_until(&mut cursor, &[], None).expect(text),
            ZoneChangeUntilNode::SourceLeavesBattlefield
        );
        cursor.finish().expect("the whole duration is consumed");
    }
}

#[test]
fn turn_durations_and_other_events_are_not_claimed() {
    for text in [
        "until end of turn",
        "until your next turn",
        "until the end of your next turn",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("until-test", &tokens);
        assert!(!zone_change_until_starts(&cursor), "{text}");
    }
    let tokens = recognize_entities("until this creature leaves your graveyard");
    let mut cursor = OracleTokenCursor::new("until-test", &tokens);
    let failure = recognize_zone_change_until(&mut cursor, &[], None)
        .expect_err("only the battlefield departure is a supported ending event");
    assert_eq!(failure.state, "zone-change.until.zone");
}
