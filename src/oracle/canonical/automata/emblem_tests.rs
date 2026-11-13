use crate::oracle::syntax::recognize_entities;

use super::{
    LocalPlayerReference, OracleTokenCursor, PlayerModifierNode, recognize_create_emblem_effect,
};

#[test]
fn emblem_envelope_contains_an_independent_player_modifier() {
    let tokens = recognize_entities("You get an emblem with \"You have no maximum hand size.\"");
    let mut cursor = OracleTokenCursor::new("emblem-test", &tokens);
    let emblem = recognize_create_emblem_effect(&mut cursor).expect("emblem parses");

    assert_eq!(emblem.player, LocalPlayerReference::Controller);
    assert_eq!(
        emblem.modifiers,
        vec![PlayerModifierNode::NoMaximumHandSize {
            player: LocalPlayerReference::Controller,
        }]
    );
    assert_eq!(cursor.position(), tokens.len());
}
