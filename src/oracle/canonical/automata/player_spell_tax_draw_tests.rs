use crate::oracle::syntax::recognize_entities;

use super::super::PlayerPredicate;
use super::{
    OracleTokenCursor, recognize_optional_draw_unless_payment, recognize_player_casts_spell_event,
};

#[test]
fn player_cast_event_retains_any_player_scope_and_triggering_binding() {
    let tokens = recognize_entities("a player casts a spell");
    let mut cursor = OracleTokenCursor::new("player-spell-cast-event-test", &tokens);

    let event = recognize_player_casts_spell_event(&mut cursor).expect("event parses");

    assert_eq!(event.caster, PlayerPredicate::Any);
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn that_player_requires_a_player_bearing_event_antecedent() {
    let tokens = recognize_entities("you may draw a card unless that player pays {1}");
    let mut cursor = OracleTokenCursor::new("player-spell-tax-draw-test", &tokens);

    let failure = recognize_optional_draw_unless_payment(&mut cursor, None)
        .expect_err("an unbound `that player` must fail");

    assert_eq!(failure.state, "reference.triggering-player.binding");
    assert_eq!(failure.actual.as_deref(), Some("pays"));
    assert_eq!(
        failure.expected,
        "a prior triggering-player binding for `that player`"
    );
}

#[test]
fn bare_tax_projection_cannot_drop_a_filter_or_ordinal() {
    for text in [
        "a player casts a creature spell",
        "an opponent casts their second spell each turn",
        "you cast a spell",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("bare-tax-boundary", &tokens);
        recognize_player_casts_spell_event(&mut cursor)
            .expect_err("the bare-tax view cannot discard event properties");
    }
}
