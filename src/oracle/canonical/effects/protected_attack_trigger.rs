use super::super::*;

fn lower_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            unreachable!("protected-attack grammar binds the ability controller")
        }
    }
}

fn lower_attack_defender(predicate: &AttackDefenderPredicateNode) -> Value {
    match predicate {
        AttackDefenderPredicateNode::Player(player) => json!({
            "kind": "player",
            "player": lower_player(player),
        }),
        AttackDefenderPredicateNode::Permanent {
            predicate,
            controller: ObjectControllerScope::AbilityController,
        } => json!({
            "kind": "permanent",
            "controller": controller(),
            "where": lower_permanent_object_predicate(predicate),
        }),
        AttackDefenderPredicateNode::Permanent {
            controller:
                ObjectControllerScope::OpponentOfAbilityController
                | ObjectControllerScope::NotAbilityController
                | ObjectControllerScope::EventPlayer,
            ..
        } => unreachable!("protected-attack grammar binds controlled planeswalkers"),
        AttackDefenderPredicateNode::Union(operands) => json!({
            "kind": "union",
            "operands": operands.iter().map(lower_attack_defender).collect::<Vec<_>>(),
        }),
    }
}

fn lower_duration(duration: &EffectDurationNode) -> Value {
    match duration {
        EffectDurationNode::UntilNextTurn { player } => json!({
            "kind": "untilNextTurn",
            "player": lower_player(player),
        }),
        EffectDurationNode::UntilEndOfCurrentTurn => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        EffectDurationNode::UntilEndOfNextTurn { .. }
        | EffectDurationNode::WhileSourceControlled
        | EffectDurationNode::Indefinite => {
            unreachable!("protected-attack grammar reads only the continuous-effect durations")
        }
    }
}

pub(in crate::oracle::canonical) fn lower_temporary_protected_attack_trigger(
    trigger: &TemporaryProtectedAttackTriggerNode,
) -> Value {
    json!({
        "kind": "installDelayedTriggeredAbility",
        "controller": lower_player(&trigger.controller),
        "duration": lower_duration(&trigger.duration),
        "event": {
            "kind": "permanentAttacks",
            "attacker": lower_permanent_object_predicate(&trigger.event.attacker),
            "defender": lower_attack_defender(&trigger.event.defender),
        },
        "effects": trigger.effects.iter().map(|effect| json!({
            "kind": "modifyPowerToughness",
            "object": match effect.object {
                TriggeringObjectReferenceNode::TriggeringPermanent => {
                    json!({ "kind": "triggeringPermanent" })
                }
            },
            "power": integer(effect.power),
            "toughness": integer(effect.toughness),
            "duration": lower_duration(&effect.duration),
        })).collect::<Vec<_>>(),
    })
}
