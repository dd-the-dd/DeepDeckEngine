use crate::oracle::syntax::recognize_entities;

use super::super::{EffectDurationNode, ObjectCardType};
use super::{
    AttackDefenderPredicateNode, LocalPlayerReference, ObjectControllerScope, ObjectPredicate,
    OracleTokenCursor, TriggeringObjectReferenceNode, recognize_temporary_protected_attack_trigger,
};

#[test]
fn protected_attack_trigger_is_duration_event_union_and_atomic_modifier() {
    let tokens = recognize_entities(
        "Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -1/-0 until end of turn.",
    );
    let mut cursor = OracleTokenCursor::new("protected-attack-test", &tokens);
    let trigger = recognize_temporary_protected_attack_trigger(&mut cursor)
        .expect("protected attack trigger parses");

    assert_eq!(
        trigger.duration,
        EffectDurationNode::UntilNextTurn {
            player: LocalPlayerReference::Controller,
        }
    );
    assert_eq!(
        trigger.event.attacker,
        ObjectPredicate::CardType(ObjectCardType::Creature)
    );
    assert_eq!(
        trigger.event.defender,
        AttackDefenderPredicateNode::Union(vec![
            AttackDefenderPredicateNode::Player(LocalPlayerReference::Controller),
            AttackDefenderPredicateNode::Permanent {
                predicate: ObjectPredicate::CardType(ObjectCardType::Planeswalker),
                controller: ObjectControllerScope::AbilityController,
            },
        ])
    );
    assert_eq!(trigger.effects.len(), 1);
    assert_eq!(
        trigger.effects[0].object,
        TriggeringObjectReferenceNode::TriggeringPermanent
    );
    assert_eq!(
        (trigger.effects[0].power, trigger.effects[0].toughness),
        (-1, 0)
    );
    assert_eq!(cursor.position(), tokens.len());
}
