use crate::oracle::characteristic_vocabulary::{AcceptedForms, CardType, lookup_card_type};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, LocalPlayerReference, ObjectControllerScope, ObjectPredicate,
    recognize_effect_duration, recognize_signed_power_toughness_delta,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TriggeringObjectReferenceNode {
    TriggeringPermanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttackDefenderPredicateNode {
    Player(LocalPlayerReference),
    Permanent {
        predicate: ObjectPredicate,
        controller: ObjectControllerScope,
    },
    Union(Vec<AttackDefenderPredicateNode>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentAttacksEventNode {
    pub attacker: ObjectPredicate,
    pub defender: AttackDefenderPredicateNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ModifyTriggeringPowerToughnessNode {
    pub object: TriggeringObjectReferenceNode,
    pub power: i64,
    pub toughness: i64,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryProtectedAttackTriggerNode {
    pub duration: EffectDurationNode,
    pub controller: LocalPlayerReference,
    pub event: PermanentAttacksEventNode,
    pub effects: Vec<ModifyTriggeringPowerToughnessNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProtectedAttackEventState {
    Trigger,
    AttackerArticle,
    AttackerType,
    Action,
    ProtectedPlayer,
    Union,
    PlaneswalkerArticle,
    PlaneswalkerType,
    Controller,
    Relation,
    Separator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_protected_attack_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermanentAttacksEventNode> {
    let mut state = ProtectedAttackEventState::Trigger;
    loop {
        state = match state {
            ProtectedAttackEventState::Trigger => {
                cursor.enter("event.protected-attack.trigger");
                cursor.consume("whenever")?;
                ProtectedAttackEventState::AttackerArticle
            }
            ProtectedAttackEventState::AttackerArticle => {
                cursor.enter("event.protected-attack.attacker-article");
                cursor.consume("a")?;
                ProtectedAttackEventState::AttackerType
            }
            ProtectedAttackEventState::AttackerType => {
                cursor.enter("event.protected-attack.attacker-type");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a registered attacking card type"))?
                    .to_string();
                let card_type = lookup_card_type(&raw, AcceptedForms::SingularOnly)
                    .ok_or_else(|| cursor.fail("a registered attacking card type"))?;
                cursor.require(
                    card_type == CardType::Creature,
                    "the creature attacker domain",
                )?;
                cursor.consume(&raw)?;
                ProtectedAttackEventState::Action
            }
            ProtectedAttackEventState::Action => {
                cursor.enter("event.protected-attack.action");
                cursor.consume("attacks")?;
                ProtectedAttackEventState::ProtectedPlayer
            }
            ProtectedAttackEventState::ProtectedPlayer => {
                cursor.enter("event.protected-attack.protected-player");
                cursor.consume("you")?;
                ProtectedAttackEventState::Union
            }
            ProtectedAttackEventState::Union => {
                cursor.enter("event.protected-attack.recipient-union");
                cursor.consume("or")?;
                ProtectedAttackEventState::PlaneswalkerArticle
            }
            ProtectedAttackEventState::PlaneswalkerArticle => {
                cursor.enter("event.protected-attack.planeswalker-article");
                cursor.consume("a")?;
                ProtectedAttackEventState::PlaneswalkerType
            }
            ProtectedAttackEventState::PlaneswalkerType => {
                cursor.enter("event.protected-attack.planeswalker-type");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("the planeswalker protected domain"))?
                    .to_string();
                cursor.require(
                    lookup_card_type(&raw, AcceptedForms::SingularOnly)
                        == Some(CardType::Planeswalker),
                    "the planeswalker protected domain",
                )?;
                cursor.consume(&raw)?;
                ProtectedAttackEventState::Controller
            }
            ProtectedAttackEventState::Controller => {
                cursor.enter("event.protected-attack.controller");
                cursor.consume("you")?;
                ProtectedAttackEventState::Relation
            }
            ProtectedAttackEventState::Relation => {
                cursor.enter("event.protected-attack.control-relation");
                cursor.consume("control")?;
                ProtectedAttackEventState::Separator
            }
            ProtectedAttackEventState::Separator => {
                cursor.enter("event.protected-attack.separator");
                cursor.consume(",")?;
                ProtectedAttackEventState::Complete
            }
            ProtectedAttackEventState::Complete => {
                cursor.enter("event.protected-attack.complete");
                return Ok(PermanentAttacksEventNode {
                    attacker: ObjectPredicate::CardType(CardType::Creature),
                    defender: AttackDefenderPredicateNode::Union(vec![
                        AttackDefenderPredicateNode::Player(LocalPlayerReference::Controller),
                        AttackDefenderPredicateNode::Permanent {
                            predicate: ObjectPredicate::CardType(CardType::Planeswalker),
                            controller: ObjectControllerScope::AbilityController,
                        },
                    ]),
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModifyStatsState {
    Object,
    Action,
    Statistics,
    Duration,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_triggering_power_toughness_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ModifyTriggeringPowerToughnessNode> {
    let mut state = ModifyStatsState::Object;
    let mut statistics = None;
    let mut duration = None;
    loop {
        state = match state {
            ModifyStatsState::Object => {
                cursor.enter("reference.triggering-permanent");
                cursor.consume("it")?;
                ModifyStatsState::Action
            }
            ModifyStatsState::Action => {
                cursor.enter("effect.modify-power-toughness.action");
                cursor.consume("gets")?;
                ModifyStatsState::Statistics
            }
            ModifyStatsState::Statistics => {
                cursor.enter("effect.modify-power-toughness.statistics");
                statistics = Some(recognize_signed_power_toughness_delta(cursor)?);
                ModifyStatsState::Duration
            }
            ModifyStatsState::Duration => {
                cursor.enter("effect.modify-power-toughness.duration");
                duration = Some(recognize_effect_duration(cursor)?);
                ModifyStatsState::Terminator
            }
            ModifyStatsState::Terminator => {
                cursor.enter("effect.modify-power-toughness.terminator");
                cursor.consume(".")?;
                ModifyStatsState::Complete
            }
            ModifyStatsState::Complete => {
                cursor.enter("effect.modify-power-toughness.complete");
                let statistics =
                    cursor.require_value(statistics, "populated power/toughness modifiers")?;
                return Ok(ModifyTriggeringPowerToughnessNode {
                    object: TriggeringObjectReferenceNode::TriggeringPermanent,
                    power: statistics.power,
                    toughness: statistics.toughness,
                    duration: cursor.require_value(duration, "a modifier duration")?,
                });
            }
        };
    }
}

/// Composes one duration, one attack event and one independent triggered
/// modifier. The result is an installation envelope, not a combined engine
/// operation.
pub(in crate::oracle::canonical) fn recognize_temporary_protected_attack_trigger(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TemporaryProtectedAttackTriggerNode> {
    cursor.enter("temporary-trigger.duration");
    let duration = recognize_effect_duration(cursor)?;
    cursor.enter("temporary-trigger.duration-separator");
    cursor.consume(",")?;
    let event = recognize_protected_attack_event(cursor)?;
    let effect = recognize_triggering_power_toughness_modifier(cursor)?;
    Ok(TemporaryProtectedAttackTriggerNode {
        duration,
        controller: LocalPlayerReference::Controller,
        event,
        effects: vec![effect],
    })
}

#[cfg(test)]
#[path = "protected_attack_trigger_tests.rs"]
mod tests;
