use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostReductionNode, ActivationCostSequenceNode, EffectSequenceNode,
    recognize_activation_cost_reduction, recognize_activation_cost_sequence,
    recognize_effect_sequence_before_activation_reduction,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ShortAbilityLabelNode {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LabeledActivatedEffectSequenceNode {
    pub label: Option<ShortAbilityLabelNode>,
    pub costs: ActivationCostSequenceNode,
    pub effects: EffectSequenceNode,
    pub reduction: ActivationCostReductionNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabeledActivationState {
    Label,
    LabelSeparator,
    Costs,
    EffectSeparator,
    Effects,
    Reduction,
    Complete,
}

/// Composes a typed short label, activation costs, an ordinary effect
/// sequence, and trailing activation metadata. Effects never absorb the cost
/// reduction suffix.
pub(in crate::oracle::canonical) fn recognize_labeled_activated_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LabeledActivatedEffectSequenceNode> {
    let mut state = LabeledActivationState::Label;
    let mut label = None;
    let mut costs = None;
    let mut effects = None;
    let mut reduction = None;
    loop {
        state = match state {
            LabeledActivationState::Label => {
                // The ability boundary strips a top-level label (CR 207.2c);
                // an unlabeled envelope starts with its costs.
                cursor.enter("activation.labeled.label");
                if cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "abilityWord")
                {
                    let token = cursor.consume_type("abilityWord")?;
                    label = Some(ShortAbilityLabelNode {
                        name: token.raw.clone(),
                    });
                    LabeledActivationState::LabelSeparator
                } else {
                    LabeledActivationState::Costs
                }
            }
            LabeledActivationState::LabelSeparator => {
                cursor.enter("activation.labeled.label-separator");
                let separator = cursor.consume_type("separator")?;
                cursor.require(separator.raw == "—", "an em dash after the ability label")?;
                LabeledActivationState::Costs
            }
            LabeledActivationState::Costs => {
                cursor.enter("activation.labeled.costs");
                costs = Some(recognize_activation_cost_sequence(cursor)?);
                LabeledActivationState::EffectSeparator
            }
            LabeledActivationState::EffectSeparator => {
                cursor.enter("activation.labeled.effect-separator");
                cursor.consume(":")?;
                LabeledActivationState::Effects
            }
            LabeledActivationState::Effects => {
                cursor.enter("activation.labeled.effects");
                effects = Some(recognize_effect_sequence_before_activation_reduction(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                LabeledActivationState::Reduction
            }
            LabeledActivationState::Reduction => {
                cursor.enter("activation.labeled.reduction");
                reduction = Some(recognize_activation_cost_reduction(cursor)?);
                LabeledActivationState::Complete
            }
            LabeledActivationState::Complete => {
                cursor.enter("activation.labeled.complete");
                cursor.finish()?;
                return Ok(LabeledActivatedEffectSequenceNode {
                    label,
                    costs: cursor.require_value(costs, "populated activation costs")?,
                    effects: cursor.require_value(effects, "a populated effect sequence")?,
                    reduction: cursor
                        .require_value(reduction, "populated activation reduction metadata")?,
                });
            }
        };
    }
}
