use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivatedManaAbilityNode, ActivationCostSequenceNode, AddManaEffectNode, EffectSequenceNode,
    recognize_activation_cost_sequence, recognize_add_mana_effect, recognize_effect_sequence,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationState {
    Costs,
    Separator,
    Effect,
    EffectTerminator,
    FollowupOrComplete,
    Followup,
    Complete,
}

#[derive(Default)]
struct ActivationMemory {
    costs: Option<ActivationCostSequenceNode>,
    effect: Option<AddManaEffectNode>,
    followup: Option<EffectSequenceNode>,
}

/// Recognizes a compact mana-ability envelope composed from the shared
/// activation-cost sequence and one typed mana-production effect.
pub(in crate::oracle::canonical) fn recognize_activated_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ActivatedManaAbilityNode> {
    let mut state = ActivationState::Costs;
    let mut memory = ActivationMemory::default();
    loop {
        state = match state {
            ActivationState::Costs => {
                cursor.enter("activation.mana.costs");
                memory.costs = Some(recognize_activation_cost_sequence(cursor)?);
                ActivationState::Separator
            }
            ActivationState::Separator => {
                cursor.enter("activation.mana.separator");
                cursor.consume(":")?;
                ActivationState::Effect
            }
            ActivationState::Effect => {
                cursor.enter("activation.mana.effect");
                memory.effect = Some(recognize_add_mana_effect(cursor)?);
                ActivationState::EffectTerminator
            }
            ActivationState::EffectTerminator => {
                cursor.enter("activation.mana.terminator");
                cursor.consume(".")?;
                ActivationState::FollowupOrComplete
            }
            ActivationState::FollowupOrComplete => {
                cursor.enter("activation.mana.followup-or-complete");
                if cursor.peek().is_some() {
                    ActivationState::Followup
                } else {
                    ActivationState::Complete
                }
            }
            ActivationState::Followup => {
                cursor.enter("activation.mana.followup");
                memory.followup =
                    Some(recognize_effect_sequence(cursor, name_links, name_failure)?);
                ActivationState::Complete
            }
            ActivationState::Complete => {
                cursor.enter("activation.mana.complete");
                cursor.finish()?;
                return Ok(ActivatedManaAbilityNode {
                    costs: cursor.require_value(memory.costs, "populated activation costs")?,
                    effect: cursor.require_value(memory.effect, "a populated activation effect")?,
                    followup: memory.followup,
                });
            }
        };
    }
}
