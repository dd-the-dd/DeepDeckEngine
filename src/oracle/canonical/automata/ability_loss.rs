use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, ObjectKind, ObjectPredicate, recognize_effect_duration,
    recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectSetBinding(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundPermanentSetNode {
    pub binding: ObjectSetBinding,
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SetAbilityLossEffectNode {
    pub objects: BoundPermanentSetNode,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SetAbilityLossState {
    Quantifier,
    PermanentSet,
    LossAction,
    AbilityCollection,
    Duration,
    Complete,
}

#[derive(Default)]
struct SetAbilityLossFrame {
    predicate: Option<ObjectPredicate>,
    duration: Option<EffectDurationNode>,
}

pub(in crate::oracle::canonical) fn object_predicate_uses_x(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => right.uses_x(),
        ObjectPredicate::Not(operand) => object_predicate_uses_x(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().any(object_predicate_uses_x)
        }
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::HasKeyword(_)
        | ObjectPredicate::Status(_)
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ObjectKind(_) => false,
    }
}

/// Recognizes one independent set effect:
/// `PERMANENT_SET loses all abilities DURATION`.
///
/// Sentence punctuation and any later effect are deliberately owned by the
/// shared EffectSequence parent. The parent allocates the binding before this
/// child runs so later references can retain typed identity.
pub(in crate::oracle::canonical) fn recognize_set_ability_loss_effect(
    cursor: &mut OracleTokenCursor<'_>,
    binding: ObjectSetBinding,
) -> MachineResult<SetAbilityLossEffectNode> {
    let mut state = SetAbilityLossState::Quantifier;
    let mut frame = SetAbilityLossFrame::default();
    loop {
        state = match state {
            SetAbilityLossState::Quantifier => {
                cursor.enter("effect.set-ability-loss.quantifier");
                cursor.consume("each")?;
                SetAbilityLossState::PermanentSet
            }
            SetAbilityLossState::PermanentSet => {
                cursor.enter("effect.set-ability-loss.permanent-set");
                let query = recognize_object_query(cursor)?;
                cursor.require(query.controller.is_none(), "an unscoped permanent set")?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent)
                        && query.predicate.has_domain(ObjectKind::Permanent),
                    "a battlefield-permanent predicate",
                )?;
                frame.predicate = Some(query.predicate);
                SetAbilityLossState::LossAction
            }
            SetAbilityLossState::LossAction => {
                cursor.enter("effect.set-ability-loss.action");
                cursor.consume("loses")?;
                SetAbilityLossState::AbilityCollection
            }
            SetAbilityLossState::AbilityCollection => {
                cursor.enter("effect.set-ability-loss.ability-collection");
                cursor.consume_phrase(&["all", "abilities"])?;
                SetAbilityLossState::Duration
            }
            SetAbilityLossState::Duration => {
                cursor.enter("effect.set-ability-loss.duration");
                frame.duration = Some(recognize_effect_duration(cursor)?);
                SetAbilityLossState::Complete
            }
            SetAbilityLossState::Complete => {
                cursor.enter("effect.set-ability-loss.complete");
                return Ok(SetAbilityLossEffectNode {
                    objects: BoundPermanentSetNode {
                        binding,
                        predicate: cursor
                            .require_value(frame.predicate, "an affected permanent set")?,
                    },
                    duration: cursor.require_value(frame.duration, "an effect duration")?,
                });
            }
        };
    }
}
