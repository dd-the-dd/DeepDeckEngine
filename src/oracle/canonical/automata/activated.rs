use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivatedManaAbilityNode, ActivationCostSequenceNode, AddManaEffectNode, EffectSequenceNode,
    recognize_add_mana_effect,
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
                memory.costs = Some(super::recognize_named_activation_cost_sequence(
                    cursor, name_links,
                )?);
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
                if cursor.peek() == Some(".") {
                    cursor.consume(".")?;
                    ActivationState::FollowupOrComplete
                } else if cursor.peek().is_some_and(|raw| {
                    raw == ","
                        || raw.eq_ignore_ascii_case("and")
                        || raw.eq_ignore_ascii_case("then")
                }) {
                    // The shared connector orders independent effects; it is
                    // not part of the preceding mana-choice list.
                    cursor.enter("activation.mana.followup-connector");
                    super::recognize_effect_connector(cursor)?;
                    ActivationState::Followup
                } else {
                    return Err(cursor.fail("."));
                }
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
                // The only consumer lowers the followup through the shared
                // sequence lowerer, which owns sentence-conditional ranges.
                // The costs prove whether the battlefield source is named
                // by a followup's self-reference (`This land doesn't untap`).
                let mut context =
                    super::EffectRecognitionContext::default().with_sentence_conditionals();
                context.activation_source_availability =
                    super::ExplicitActivationSourceAvailability::from_costs(
                        cursor
                            .require_value(memory.costs.as_ref(), "populated activation costs")?,
                        false,
                    );
                memory.followup = Some(super::recognize_effect_sequence_with_context(
                    cursor,
                    name_links,
                    name_failure,
                    &context,
                )?);
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

#[cfg(test)]
#[path = "activated_mana_sequence_boundary_tests.rs"]
mod sequence_boundary_tests;
