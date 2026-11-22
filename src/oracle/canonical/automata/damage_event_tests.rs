use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn read(text: &str) -> MachineResult<DamageDealtComplement> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("damage-complement-test", &tokens);
    recognize_damage_dealt_complement(&mut cursor)
}

#[test]
fn damage_kind_and_recipient_are_independent() {
    for (text, kind, player) in [
        (
            "damage to an opponent",
            DamageKind::Any,
            EventActorScope::Opponent,
        ),
        (
            "combat damage to a player",
            DamageKind::Combat,
            EventActorScope::AnyPlayer,
        ),
        (
            "noncombat damage to you",
            DamageKind::Noncombat,
            EventActorScope::Controller,
        ),
    ] {
        let complement = read(text).unwrap_or_else(|failure| panic!("{text}: {failure}"));
        assert_eq!(complement.kind, kind, "{text}");
        assert_eq!(
            complement.recipient,
            Some(DamageRecipient::Player(player)),
            "{text}"
        );
    }
    assert!(
        read("combat damage to a player")
            .unwrap()
            .is_combat_damage_to_any_player()
    );
    assert!(
        !read("damage to a player")
            .unwrap()
            .is_combat_damage_to_any_player()
    );
    let creature = read("combat damage to a creature").unwrap();
    assert!(matches!(
        creature.recipient,
        Some(DamageRecipient::Permanent(_))
    ));
    assert_eq!(read("damage").unwrap().recipient, None);
}

#[test]
fn a_damage_recipient_is_a_player_or_a_permanent() {
    let failure = read("damage to target creature").unwrap_err();
    assert_eq!(failure.state, "event.dealt-damage.recipient");
    let failure = read("damage to a creature card").unwrap_err();
    assert!(
        failure.state.starts_with("event.dealt-damage.recipient"),
        "{failure}"
    );
}
