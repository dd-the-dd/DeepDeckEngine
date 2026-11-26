use crate::oracle::syntax::recognize_entities;

use super::{
    LocalPlayerReference, LocalTriggerEventNode, OracleTokenCursor, recognize_land_played_event,
};

#[test]
fn another_land_retains_source_exclusion() {
    let tokens = recognize_entities("you play another land");
    let mut cursor = OracleTokenCursor::new("test.land-played", &tokens);

    let event = recognize_land_played_event(&mut cursor).expect("land event parses");

    assert_eq!(
        event,
        LocalTriggerEventNode::LandPlayed {
            player: LocalPlayerReference::Controller,
            exclude_source: true,
            from_exile: false,
        }
    );
}

#[test]
fn indefinite_land_does_not_exclude_the_source() {
    let tokens = recognize_entities("you play a land");
    let mut cursor = OracleTokenCursor::new("test.land-played-indefinite", &tokens);

    let event = recognize_land_played_event(&mut cursor).expect("land event parses");

    assert_eq!(
        event,
        LocalTriggerEventNode::LandPlayed {
            player: LocalPlayerReference::Controller,
            exclude_source: false,
            from_exile: false,
        }
    );
}

#[test]
fn another_creature_fails_at_the_object_type_state() {
    let tokens = recognize_entities("you play another creature");
    let mut cursor = OracleTokenCursor::new("test.land-played-wrong-type", &tokens);

    let failure = recognize_land_played_event(&mut cursor)
        .expect_err("another creature is not a land-play event");

    assert_eq!(failure.state, "event.land-played.object-type");
    assert_eq!(failure.actual.as_deref(), Some("creature"));
}

#[test]
fn a_land_played_from_exile_keeps_its_origin() {
    let tokens = recognize_entities("you play a land from exile or cast a spell from exile");
    let mut cursor = OracleTokenCursor::new("test.land-played-from-exile", &tokens);

    let event = recognize_land_played_event(&mut cursor).expect("land event parses");

    assert_eq!(
        event,
        LocalTriggerEventNode::LandPlayed {
            player: LocalPlayerReference::Controller,
            exclude_source: false,
            from_exile: true,
        }
    );
    assert_eq!(cursor.peek(), Some("or"), "the union belongs to the clause");
}

#[test]
fn a_land_played_from_another_zone_fails_at_the_origin() {
    let tokens = recognize_entities("you play a land from your graveyard");
    let mut cursor = OracleTokenCursor::new("test.land-played-from-graveyard", &tokens);

    let failure = recognize_land_played_event(&mut cursor)
        .expect_err("only exile is a land-play origin with a matcher");

    assert_eq!(failure.state, "event.land-played.origin");
}
