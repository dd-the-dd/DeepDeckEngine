use super::*;
use crate::oracle::syntax::recognize_entities;

fn recognize(text: &str) -> MachineResult<LocalEffectNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-damage-recipient", &tokens);
    let effect = recognize_damage_effect(&mut cursor, &[], None)?;
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(effect)
}

#[test]
fn damage_reuses_shared_opponent_and_all_player_populations() {
    for (text, expected) in [
        (
            "This spell deals 2 damage to each opponent.",
            super::super::PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
        (
            "This spell deals 3 damage to each player.",
            super::super::PlayerSetReferenceNode::EachPlayer,
        ),
        (
            "This spell deals 4 damage to your opponents.",
            super::super::PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
    ] {
        let LocalEffectNode::DealDamage(effect) = recognize(text).unwrap() else {
            panic!("expected typed damage action");
        };
        assert_eq!(effect.recipient, DamageRecipientNode::PlayerSet(expected));
    }
}

#[test]
fn player_population_does_not_absorb_an_extra_role_qualifier_or_effect() {
    for text in [
        "This spell deals 2 damage to each opponents.",
        "This spell deals 2 damage to each player or opponent.",
        "This spell deals 2 damage to each opponent other than you.",
        "This spell deals 2 damage to your opponents and gains 2 life.",
        "It deals 2 damage to each opponent.",
    ] {
        assert!(recognize(text).is_err(), "{text}");
    }
}

#[test]
fn existing_permanent_collection_and_single_player_domains_are_unchanged() {
    assert!(matches!(
        recognize("This spell deals 2 damage to each creature.").unwrap(),
        LocalEffectNode::DealDamage(DamageEffectNode {
            recipient: DamageRecipientNode::EachPermanent(_),
            ..
        })
    ));
    assert!(matches!(
        recognize("This spell deals 2 damage to you.").unwrap(),
        LocalEffectNode::DealDamage(DamageEffectNode {
            recipient: DamageRecipientNode::Player(_),
            ..
        })
    ));
}
