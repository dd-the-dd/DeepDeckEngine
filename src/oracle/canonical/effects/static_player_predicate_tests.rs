//! Each player predicate lowers to one primitive on the same population.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn lowered(text: &str, face_type_line: &str) -> MachineResult<Vec<Value>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("static-player-lowering-test", &tokens);
    let reading = recognize_static_player_subject(&mut cursor)?;
    let players = lower_static_player_subject(&cursor, reading.subject, face_type_line)?;
    let predicate = recognize_static_player_predicate(&mut cursor, reading)?;
    lower_static_player_predicate(&mut cursor, &players, &predicate)
}

#[test]
fn cast_prohibitions_keep_spell_filter_and_window_as_separate_fields() {
    let opponents = json!({ "kind": "opponentsOf", "player": controller() });
    assert_eq!(
        lowered(
            "Your opponents can't cast spells during your turn.",
            "Creature"
        )
        .unwrap(),
        vec![json!({
            "kind": "prohibitCast", "players": opponents, "duringTurnOf": controller(),
        })]
    );
    let modifiers = lowered(
        "Players can't cast creature spells during combat.",
        "Creature",
    )
    .unwrap();
    assert_eq!(modifiers[0]["duringCombat"], json!(true));
    assert_eq!(
        modifiers[0]["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
}

#[test]
fn attached_controller_requires_an_attachment_source() {
    assert_eq!(
        lowered(
            "Enchanted creature's controller can't gain life.",
            "Enchantment - Aura"
        )
        .unwrap()[0]["players"],
        json!({ "kind": "controllerOfAttachedPermanent", "attachment": self_ref() })
    );
    assert!(
        lowered(
            "Enchanted creature's controller can't gain life.",
            "Enchantment"
        )
        .is_err()
    );
}
