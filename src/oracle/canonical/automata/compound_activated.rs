use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DiscardSelfCostNode, ManaCostNode, PayLifeCostNode, SacrificeSelfCostNode,
    SelectedSacrificeCostNode, TapSelfCostNode, ZoneChangeEffectNode, object_predicate_uses_x,
    recognize_discard_self_cost, recognize_mana_cost, recognize_pay_life_cost,
    recognize_sacrifice_self_cost, recognize_selected_sacrifice_cost, recognize_tap_self_cost,
    recognize_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LocalActivationCostNode {
    PayMana(ManaCostNode),
    TapSelf(TapSelfCostNode),
    PayLife(PayLifeCostNode),
    SacrificeSelf(SacrificeSelfCostNode),
    SacrificeObject(SelectedSacrificeCostNode),
    DiscardSelf(DiscardSelfCostNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationCostConnector {
    Comma,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationCostSequenceNode {
    pub costs: Vec<LocalActivationCostNode>,
    pub connectors: Vec<ActivationCostConnector>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationSourceZoneRequirements {
    pub hand: bool,
    pub battlefield: bool,
}

impl ActivationCostSequenceNode {
    pub(in crate::oracle::canonical) fn source_zone_requirements(
        &self,
    ) -> ActivationSourceZoneRequirements {
        let mut requirements = ActivationSourceZoneRequirements::default();
        for cost in &self.costs {
            match cost {
                LocalActivationCostNode::DiscardSelf(_) => requirements.hand = true,
                LocalActivationCostNode::TapSelf(_) | LocalActivationCostNode::SacrificeSelf(_) => {
                    requirements.battlefield = true
                }
                LocalActivationCostNode::PayMana(_)
                | LocalActivationCostNode::PayLife(_)
                | LocalActivationCostNode::SacrificeObject(_) => {}
            }
        }
        requirements
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CompoundActivatedZoneChangeNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: ZoneChangeEffectNode,
}

pub(in crate::oracle::canonical) enum CompoundActivatedZoneChangeParseAttempt {
    Parsed(CompoundActivatedZoneChangeNode),
    RejectedCandidate(super::super::MachineFailure),
    FailedCommitted(super::super::MachineFailure),
}

enum CompoundActivatedZoneChangeParseFailure {
    UnsupportedEnvelope(super::super::MachineFailure),
    Malformed(super::super::MachineFailure),
}

impl From<super::super::MachineFailure> for CompoundActivatedZoneChangeParseFailure {
    fn from(failure: super::super::MachineFailure) -> Self {
        Self::Malformed(failure)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedState {
    FirstCost,
    CostContinuation,
    EffectSeparator,
    Effect,
    Terminator,
    Complete,
}

#[derive(Default)]
struct ActivatedAttemptFrame {
    costs: Vec<LocalActivationCostNode>,
    connectors: Vec<ActivationCostConnector>,
    effect: Option<ZoneChangeEffectNode>,
}

/// Recognizes a reusable activated-ability envelope composed from independent
/// cost children and one targeted battlefield zone-change effect.
pub(in crate::oracle::canonical) fn recognize_compound_activated_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CompoundActivatedZoneChangeNode> {
    recognize_compound_activated_zone_change_internal(cursor).map_err(|failure| match failure {
        CompoundActivatedZoneChangeParseFailure::UnsupportedEnvelope(failure)
        | CompoundActivatedZoneChangeParseFailure::Malformed(failure) => failure,
    })
}

/// A parsed prefix is not an atomic-family commit. A complete selector followed
/// by another effect belongs to the shared program parent, while malformed
/// costs, targets and destinations remain committed failures in this family.
pub(in crate::oracle::canonical) fn try_recognize_compound_activated_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> CompoundActivatedZoneChangeParseAttempt {
    let checkpoint = cursor.checkpoint();
    match recognize_compound_activated_zone_change_internal(cursor) {
        Ok(node) => CompoundActivatedZoneChangeParseAttempt::Parsed(node),
        Err(CompoundActivatedZoneChangeParseFailure::UnsupportedEnvelope(failure)) => {
            cursor.rollback(checkpoint);
            CompoundActivatedZoneChangeParseAttempt::RejectedCandidate(failure)
        }
        Err(CompoundActivatedZoneChangeParseFailure::Malformed(failure)) => {
            CompoundActivatedZoneChangeParseAttempt::FailedCommitted(failure)
        }
    }
}

fn recognize_compound_activated_zone_change_internal(
    cursor: &mut OracleTokenCursor<'_>,
) -> Result<CompoundActivatedZoneChangeNode, CompoundActivatedZoneChangeParseFailure> {
    let costs = recognize_activation_cost_sequence(cursor)?;
    let mut state = ActivatedState::EffectSeparator;
    let mut frame = ActivatedAttemptFrame::default();
    frame.costs = costs.costs;
    frame.connectors = costs.connectors;
    loop {
        state = match state {
            ActivatedState::FirstCost | ActivatedState::CostContinuation => {
                unreachable!("the shared activation-cost sequence owns cost recognition")
            }
            ActivatedState::EffectSeparator => {
                cursor.enter("activation.compound.effect-separator");
                cursor.consume(":")?;
                ActivatedState::Effect
            }
            ActivatedState::Effect => {
                cursor.enter("activation.compound.effect");
                let effect = recognize_zone_change_effect(cursor)?;
                let super::LocalEffectNode::ZoneChange(effect) = effect else {
                    unreachable!("zone-change child always returns its typed variant")
                };
                frame.effect = Some(effect);
                ActivatedState::Terminator
            }
            ActivatedState::Terminator => {
                cursor.enter("activation.compound.terminator");
                if cursor.peek().is_some_and(|raw| raw != ".") {
                    return Err(
                        CompoundActivatedZoneChangeParseFailure::UnsupportedEnvelope(
                            cursor.fail("one atomic targeted zone-change effect"),
                        ),
                    );
                }
                cursor.consume(".")?;
                ActivatedState::Complete
            }
            ActivatedState::Complete => {
                cursor.enter("activation.compound.complete");
                cursor
                    .finish()
                    .map_err(CompoundActivatedZoneChangeParseFailure::UnsupportedEnvelope)?;
                cursor.require(
                    frame.costs.len() == frame.connectors.len() + 1,
                    "one connector between each activation cost",
                )?;
                return Ok(CompoundActivatedZoneChangeNode {
                    costs: ActivationCostSequenceNode {
                        costs: frame.costs,
                        connectors: frame.connectors,
                    },
                    effect: cursor.require_value(frame.effect, "a populated zone-change effect")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_activation_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostSequenceNode> {
    let mut state = ActivatedState::FirstCost;
    let mut costs = Vec::new();
    let mut connectors = Vec::new();
    loop {
        state = match state {
            ActivatedState::FirstCost => {
                cursor.enter("activation.compound.first-cost");
                costs.push(recognize_activation_cost(cursor, costs.len())?);
                ActivatedState::CostContinuation
            }
            ActivatedState::CostContinuation => {
                cursor.enter("activation.compound.cost-continuation");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    connectors.push(ActivationCostConnector::Comma);
                    costs.push(recognize_activation_cost(cursor, costs.len())?);
                    ActivatedState::CostContinuation
                } else {
                    cursor.require(
                        costs.len() == connectors.len() + 1,
                        "one connector between each activation cost",
                    )?;
                    let costs = ActivationCostSequenceNode { costs, connectors };
                    if costs
                        .costs
                        .iter()
                        .any(|cost| matches!(cost, LocalActivationCostNode::SacrificeObject(_)))
                    {
                        let source = costs.source_zone_requirements();
                        cursor.enter("activation.compound.source-zone-conflict");
                        cursor.require(
                            !(source.hand && source.battlefield),
                            "one coherent source zone for all activation costs",
                        )?;
                    }
                    if costs.costs.iter().any(|cost| {
                        matches!(cost, LocalActivationCostNode::SacrificeObject(selected)
                            if object_predicate_uses_x(&selected.object.predicate))
                    }) {
                        cursor.enter("activation.compound.selected-cost-x-producer");
                        cursor.require(
                            costs.costs.iter().any(|cost| {
                                matches!(cost, LocalActivationCostNode::PayMana(mana)
                                    if mana.contains_variable_x())
                            }),
                            "an {X} mana cost in this activation producing X for its selected-cost predicate",
                        )?;
                    }
                    return Ok(costs);
                }
            }
            _ => unreachable!("the cost-sequence child only visits cost states"),
        };
    }
}

fn recognize_activation_cost(
    cursor: &mut OracleTokenCursor<'_>,
    cost_index: usize,
) -> MachineResult<LocalActivationCostNode> {
    cursor.enter("activation.compound.cost");
    match cursor.peek_token() {
        Some(token) if token.token_type == "manaSymbol" => {
            recognize_mana_cost(cursor).map(LocalActivationCostNode::PayMana)
        }
        Some(token) if token.token_type == "tapSymbol" => {
            recognize_tap_self_cost(cursor).map(LocalActivationCostNode::TapSelf)
        }
        Some(token) if token.raw.eq_ignore_ascii_case("pay") => {
            recognize_pay_life_cost(cursor).map(LocalActivationCostNode::PayLife)
        }
        Some(token) if token.raw.eq_ignore_ascii_case("sacrifice") => {
            if cursor.peek_ahead(1).is_some_and(|raw| raw.eq_ignore_ascii_case("this")) {
                recognize_sacrifice_self_cost(cursor).map(LocalActivationCostNode::SacrificeSelf)
            } else {
                recognize_selected_sacrifice_cost(cursor, cost_index).map(LocalActivationCostNode::SacrificeObject)
            }
        }
        Some(token) if token.raw.eq_ignore_ascii_case("discard") => {
            recognize_discard_self_cost(cursor).map(LocalActivationCostNode::DiscardSelf)
        }
        _ => Err(cursor.fail(
            "a mana payment, tap-self, fixed life payment, sacrifice-self, chosen-permanent sacrifice, or discard-self activation cost",
        )),
    }
}

#[cfg(test)]
#[path = "activation_cost_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compound_zone_attempt_tests.rs"]
mod compound_zone_attempt_tests;
