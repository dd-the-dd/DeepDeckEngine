//! Activation costs (CR 602.1a, 602.2b): the shared `CostSequenceNode` of
//! ordinary payment actions, projected onto the activation cost schema
//! (`costs[]` plus declared payment choices). The projection owns the
//! activation semantics: the source zone implied by `this card` actions, the
//! ordinal of each declared choice and the X producer. An action without an
//! activation representation fails here at its own state.
//!
//! Quantity slots are the shared numeric atom. A variable quantity is bound
//! only by this activation's own `{X}` mana cost (CR 107.3, 601.2b via
//! 602.2b): without it the activation fails binding before any lowering.
use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionSelectionMethod, CostConnector, CostListGrammar, CounterHolderNode,
    DiscardSelfCostNode, ExileSelfCostNode, ExileSelfSourceZone, ManaCostNode, NumericAtom,
    ObjectPredicate, ObjectQueryNode, PayLifeCostNode, PaymentActionNode, PaymentObjectFilter,
    PaymentObjectsNode, PaymentOriginZone, PaymentSourceNoun, PlayerCounterPaymentNode,
    RemoveCountersPaymentNode, ResourcePaymentNode, SacrificeSelfCostNode, TapObjectsCostNode,
    TapSelfCostNode, literal_quantity, object_predicate_uses_x, recognize_cost_sequence,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LocalActivationCostNode {
    PayMana(ManaCostNode),
    TapSelf(TapSelfCostNode),
    PayLife(PayLifeCostNode),
    /// `pay {E}{E}`: counters removed from the activating player.
    PayPlayerCounters(PlayerCounterPaymentNode),
    SacrificeSelf(SacrificeSelfCostNode),
    SacrificeObject(SelectedSacrificeCostNode),
    DiscardSelf(DiscardSelfCostNode),
    ExileSelf(ExileSelfCostNode),
    DiscardCard(ChosenDiscardCostNode),
    TapObjects(ChosenTapCostNode),
    RemoveSourceCounters(SourceCounterRemovalCostNode),
    RemoveControlledCounters(ControlledCounterRemovalCostNode),
    ExileCards(ChosenExileCostNode),
    /// `return this PERMANENT to its owner's hand`.
    ReturnSelf,
    /// `return N PERMANENTS you control to their owner's hand`.
    ReturnObjects(ChosenReturnCostNode),
    /// `put N KIND counters on this PERMANENT`.
    PutSourceCounters(SourceCounterPlacementCostNode),
    /// `put N KIND counters on a PERMANENT you control`.
    PutChosenCounters(ChosenCounterPlacementCostNode),
}

/// Permanents the payer controls returned to hand while paying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenReturnCostNode {
    pub count: i64,
    pub object: ObjectQueryNode,
    pub exclude_source: bool,
    pub cost_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCounterPlacementCostNode {
    pub counter: CounterName,
    pub count: i64,
}

/// Counters put on one permanent the payer controls, chosen while paying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenCounterPlacementCostNode {
    pub counter: CounterName,
    pub count: i64,
    pub object: ObjectQueryNode,
    pub cost_index: usize,
}

/// Counters removed from the ability's source (on the battlefield). An
/// unnamed kind is one counter whose kind is declared while paying. The
/// count is a literal or this activation's chosen X.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCounterRemovalCostNode {
    pub counter: Option<CounterName>,
    pub count: NumericAtom,
    pub cost_index: usize,
}

/// N counters distributed while paying over the holders: permanents the
/// payer controls (one permanent when N is one) or the source alone. An
/// unnamed kind lets each removed counter be of any kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlledCounterRemovalCostNode {
    pub holder: CounterDistributionHolder,
    pub counter: Option<CounterName>,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterDistributionHolder {
    /// The ability's source (on the battlefield).
    Source,
    /// Permanents the payer controls matching the shared query.
    Permanents(ObjectQueryNode),
}

/// N cards chosen from the payer's hand or graveyard and exiled as a cost.
/// `another` excludes the ability's source card from the choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenExileCostNode {
    pub count: i64,
    pub filter: ObjectPredicate,
    pub zone: PaymentOriginZone,
    pub exclude_source: bool,
    pub cost_index: usize,
}

/// N chosen permanents sacrificed as an activation cost. The cost parent
/// assigns the stable ordinal naming the declared choice; the shared query
/// retains every characteristic filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SelectedSacrificeCostNode {
    pub count: i64,
    pub object: ObjectQueryNode,
    /// `another`: the ability's source cannot be the sacrificed permanent.
    pub exclude_source: bool,
    pub cost_index: usize,
}

/// N untapped permanents chosen from those the payer controls and tapped as a
/// cost. The cost parent assigns the ordinal naming the declared choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenTapCostNode {
    pub tap: TapObjectsCostNode,
    pub cost_index: usize,
}

/// N cards from the payer's hand discarded as a cost, chosen by the payer or
/// at random (CR 701.9b). Unlike `DiscardSelf`, it constrains no source zone:
/// the source is only the ability's host, never the discarded card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenDiscardCostNode {
    pub count: i64,
    pub filter: ObjectPredicate,
    pub method: CollectionSelectionMethod,
    pub cost_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationCostConnector {
    Comma,
    /// `Sacrifice a Swamp and a Forest`: one all-or-nothing payment.
    And,
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
    pub graveyard: bool,
}

impl ActivationCostSequenceNode {
    pub(in crate::oracle::canonical) fn source_zone_requirements(
        &self,
    ) -> ActivationSourceZoneRequirements {
        let mut requirements = ActivationSourceZoneRequirements::default();
        for cost in &self.costs {
            match cost {
                LocalActivationCostNode::DiscardSelf(_) => requirements.hand = true,
                LocalActivationCostNode::ExileSelf(exile) => match exile.source_zone {
                    ExileSelfSourceZone::Graveyard => requirements.graveyard = true,
                    ExileSelfSourceZone::Battlefield => requirements.battlefield = true,
                },
                LocalActivationCostNode::TapSelf(_)
                | LocalActivationCostNode::SacrificeSelf(_)
                | LocalActivationCostNode::ReturnSelf
                | LocalActivationCostNode::PutSourceCounters(_)
                | LocalActivationCostNode::RemoveSourceCounters(_)
                | LocalActivationCostNode::RemoveControlledCounters(
                    ControlledCounterRemovalCostNode {
                        holder: CounterDistributionHolder::Source,
                        ..
                    },
                ) => requirements.battlefield = true,
                LocalActivationCostNode::PayMana(_)
                | LocalActivationCostNode::PayLife(_)
                | LocalActivationCostNode::PayPlayerCounters(_)
                | LocalActivationCostNode::DiscardCard(_)
                | LocalActivationCostNode::TapObjects(_)
                | LocalActivationCostNode::SacrificeObject(_)
                | LocalActivationCostNode::ReturnObjects(_)
                | LocalActivationCostNode::PutChosenCounters(_)
                | LocalActivationCostNode::RemoveControlledCounters(_)
                | LocalActivationCostNode::ExileCards(_) => {}
            }
        }
        requirements
    }
}

pub(in crate::oracle::canonical) fn recognize_activation_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostSequenceNode> {
    cursor.enter("activation.compound.first-cost");
    let sequence = recognize_cost_sequence(
        cursor,
        CostListGrammar {
            comma: true,
            and: true,
        },
    )?;
    cursor.enter("activation.compound.cost-continuation");
    cursor.require(
        sequence.costs.len() == sequence.connectors.len() + 1,
        "one connector between each activation cost",
    )?;
    let connectors = sequence
        .connectors
        .iter()
        .map(|connector| match connector {
            CostConnector::Comma => ActivationCostConnector::Comma,
            CostConnector::And => ActivationCostConnector::And,
        })
        .collect();
    // X in a quantity slot is this activation's chosen X: its producer is the
    // activation's own `{X}` mana cost, checked before any projection.
    cursor.enter("activation.compound.quantity-x-producer");
    cursor.require(
        !sequence.costs.iter().any(payment_quantity_uses_x)
            || sequence.costs.iter().any(|payment| {
                matches!(payment, PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana))
                    if mana.contains_variable_x())
            }),
        "an {X} mana cost in this activation producing X for its cost quantity",
    )?;
    let costs = sequence
        .costs
        .into_iter()
        .enumerate()
        .map(|(cost_index, payment)| activation_cost_from_payment(cursor, payment, cost_index))
        .collect::<MachineResult<Vec<_>>>()?;
    let costs = ActivationCostSequenceNode { costs, connectors };
    if costs.costs.iter().any(|cost| {
        matches!(
            cost,
            LocalActivationCostNode::RemoveSourceCounters(_)
                | LocalActivationCostNode::RemoveControlledCounters(_)
        )
    }) {
        // The engine pays counter removal only for battlefield activations.
        let source = costs.source_zone_requirements();
        cursor.enter("activation.remove-counters.source-zone");
        cursor.require(
            !source.hand && !source.graveyard,
            "a battlefield activation for a counter-removal cost",
        )?;
        // The engine declares one counter distribution per activation.
        cursor.enter("activation.remove-counters.single-distribution");
        cursor.require(
            costs
                .costs
                .iter()
                .filter(|cost| matches!(cost, LocalActivationCostNode::RemoveControlledCounters(_)))
                .count()
                <= 1,
            "at most one distributed counter-removal cost per activation",
        )?;
    }
    if costs
        .costs
        .iter()
        .any(|cost| matches!(cost, LocalActivationCostNode::SacrificeObject(_)))
    {
        let source = costs.source_zone_requirements();
        cursor.enter("activation.compound.source-zone-conflict");
        cursor.require(
            !(source.hand && source.battlefield)
                && !(source.graveyard && (source.hand || source.battlefield)),
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
    Ok(costs)
}

/// Projects one payment action onto the activation cost schema. Chosen
/// objects become declared payment choices named by `cost_index`.
pub(in crate::oracle::canonical) fn activation_cost_from_payment(
    cursor: &mut OracleTokenCursor<'_>,
    payment: PaymentActionNode,
    cost_index: usize,
) -> MachineResult<LocalActivationCostNode> {
    cursor.enter("activation.compound.cost");
    match payment {
        PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana)) => {
            Ok(LocalActivationCostNode::PayMana(mana))
        }
        PaymentActionNode::Pay(ResourcePaymentNode::Life(life)) => {
            Ok(LocalActivationCostNode::PayLife(life))
        }
        PaymentActionNode::Pay(ResourcePaymentNode::PlayerCounters(counters)) => {
            Ok(LocalActivationCostNode::PayPlayerCounters(counters))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Source(PaymentSourceNoun::TapSymbol)) => {
            Ok(LocalActivationCostNode::TapSelf(TapSelfCostNode))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Source(_)) => {
            // CR 302.6 restricts {T} only; a written self-tap is another cost.
            cursor.enter("activation.tap-self.written");
            Err(cursor.fail("the {T} symbol for tapping the source as an activation cost"))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Chosen(chosen)) => {
            // `another`: the declared choice excludes the source (its
            // candidates carry `excludeSource`, honored by the engine).
            let PaymentObjectFilter::Permanents(object) = chosen.filter else {
                return Err(cursor.fail("a permanent operand"));
            };
            cursor.enter("activation.tap-objects.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&object.predicate),
                "no {X} in the tapped-permanent predicate",
            )?;
            let count = declared_object_count(cursor, chosen.count, "activation.tap-objects")?;
            Ok(LocalActivationCostNode::TapObjects(ChosenTapCostNode {
                tap: TapObjectsCostNode {
                    count,
                    object,
                    exclude_source: chosen.exclude_source,
                },
                cost_index,
            }))
        }
        PaymentActionNode::Sacrifice(PaymentObjectsNode::Source(noun)) => {
            cursor.enter("activation.sacrifice-self.source-type");
            cursor.require(
                noun == PaymentSourceNoun::Permanent,
                "a source noun naming a permanent after `sacrifice`",
            )?;
            Ok(LocalActivationCostNode::SacrificeSelf(
                SacrificeSelfCostNode,
            ))
        }
        PaymentActionNode::Sacrifice(PaymentObjectsNode::Chosen(chosen)) => {
            let PaymentObjectFilter::Permanents(object) = chosen.filter else {
                return Err(cursor.fail("a permanent operand"));
            };
            let count =
                declared_object_count(cursor, chosen.count, "activation.selected-sacrifice")?;
            Ok(LocalActivationCostNode::SacrificeObject(
                SelectedSacrificeCostNode {
                    count,
                    object,
                    exclude_source: chosen.exclude_source,
                    cost_index,
                },
            ))
        }
        PaymentActionNode::Discard(PaymentObjectsNode::Source(_)) => {
            Ok(LocalActivationCostNode::DiscardSelf(DiscardSelfCostNode))
        }
        PaymentActionNode::Discard(PaymentObjectsNode::Chosen(chosen)) => {
            // `another card` names no source card in hand: the exclusion has
            // no object to exclude in a hand discard.
            cursor.enter("activation.discard-card.source-exclusion");
            cursor.require(
                !chosen.exclude_source,
                "discarded cards without `another` or `other`",
            )?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            cursor.enter("activation.discard-card.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&filter),
                "no {X} in the discarded-card predicate",
            )?;
            let count = declared_object_count(cursor, chosen.count, "activation.discard-card")?;
            Ok(LocalActivationCostNode::DiscardCard(
                ChosenDiscardCostNode {
                    count,
                    filter,
                    method: chosen.method,
                    cost_index,
                },
            ))
        }
        PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Source(noun),
            origin,
        } => {
            // The named zone is where the ability is activated from; a
            // source permanent (`exile this artifact`) is on the battlefield.
            cursor.enter("activation.exile-self.origin");
            let source_zone = match (noun, origin) {
                (PaymentSourceNoun::Permanent, None) => ExileSelfSourceZone::Battlefield,
                (PaymentSourceNoun::Card, Some(PaymentOriginZone::Graveyard)) => {
                    ExileSelfSourceZone::Graveyard
                }
                (PaymentSourceNoun::Card, None) => {
                    return Err(cursor.fail("`from your graveyard` after `exile this card`"));
                }
                _ => {
                    cursor.enter("activation.exile-self.source-zone");
                    return Err(cursor.fail(
                        "`graveyard` for `this card` or no zone for a source permanent (the source zones an exile-self activation cost supports)",
                    ));
                }
            };
            Ok(LocalActivationCostNode::ExileSelf(ExileSelfCostNode {
                source_zone,
            }))
        }
        PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Chosen(chosen),
            origin,
        } => {
            cursor.enter("activation.exile-cards.origin");
            let zone = cursor.require_value(origin, "`from your hand` or `from your graveyard`")?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            cursor.enter("activation.exile-cards.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&filter),
                "no {X} in the exiled-card predicate",
            )?;
            cursor.enter("activation.exile-cards.selection-support");
            cursor.require(
                chosen.method == CollectionSelectionMethod::Choice,
                "payer-chosen exiled cards (a random cost exile is unsupported)",
            )?;
            let count = declared_object_count(cursor, chosen.count, "activation.exile-cards")?;
            Ok(LocalActivationCostNode::ExileCards(ChosenExileCostNode {
                count,
                filter,
                zone,
                exclude_source: chosen.exclude_source,
                cost_index,
            }))
        }
        PaymentActionNode::RemoveCounters(removal) => {
            activation_counter_removal(cursor, removal, cost_index)
        }
        PaymentActionNode::Blight { .. } => {
            cursor.enter("activation.blight");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
                "an activation cost the engine can pay (blight has no activation cost schema)",
            ))
        }
        PaymentActionNode::ReturnToHand(PaymentObjectsNode::Source(noun)) => {
            cursor.enter("activation.return-self.source-type");
            cursor.require(
                noun == PaymentSourceNoun::Permanent,
                "a source noun naming a permanent after `return`",
            )?;
            Ok(LocalActivationCostNode::ReturnSelf)
        }
        PaymentActionNode::ReturnToHand(PaymentObjectsNode::Chosen(chosen)) => {
            let PaymentObjectFilter::Permanents(object) = chosen.filter else {
                return Err(cursor.fail("a permanent operand"));
            };
            cursor.enter("activation.return-objects.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&object.predicate),
                "no {X} in the returned-permanent predicate",
            )?;
            let count = declared_object_count(cursor, chosen.count, "activation.return-objects")?;
            Ok(LocalActivationCostNode::ReturnObjects(
                ChosenReturnCostNode {
                    count,
                    object,
                    exclude_source: chosen.exclude_source,
                    cost_index,
                },
            ))
        }
        PaymentActionNode::PutCounters(placement) => match placement.holder {
            CounterHolderNode::Source(PaymentSourceNoun::Permanent) => {
                let count = literal_counter_count(cursor, placement.count)?;
                Ok(LocalActivationCostNode::PutSourceCounters(
                    SourceCounterPlacementCostNode {
                        counter: placement.counter,
                        count,
                    },
                ))
            }
            CounterHolderNode::One(object) => {
                cursor.enter("activation.put-counters.numeric-context");
                cursor.require(
                    !object_predicate_uses_x(&object.predicate),
                    "no {X} in the counter-holder predicate",
                )?;
                let count = literal_counter_count(cursor, placement.count)?;
                Ok(LocalActivationCostNode::PutChosenCounters(
                    ChosenCounterPlacementCostNode {
                        counter: placement.counter,
                        count,
                        object,
                        cost_index,
                    },
                ))
            }
            CounterHolderNode::Source(_) | CounterHolderNode::Among(_) => {
                cursor.enter("activation.put-counters.holder");
                Err(cursor.fail("the source permanent or one permanent you control"))
            }
        },
        // The engine has no activation-cost program for these actions; the
        // cost fails here, explicitly, instead of being approximated.
        PaymentActionNode::Untap(_)
        | PaymentActionNode::Reveal { .. }
        | PaymentActionNode::Mill { .. }
        | PaymentActionNode::Exert(_) => {
            cursor.enter("activation.unsupported-action");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
                "an activation cost the engine can pay (untap, reveal, mill and exert have no activation cost schema)",
            ))
        }
    }
}

/// Projects counter removal onto the engine's two activation schemas: counters
/// of one kind on the source (`removeCounters`) and counters distributed over
/// holders while paying (`removeCountersFromControlledPermanents`).
fn activation_counter_removal(
    cursor: &mut OracleTokenCursor<'_>,
    removal: RemoveCountersPaymentNode,
    cost_index: usize,
) -> MachineResult<LocalActivationCostNode> {
    let RemoveCountersPaymentNode {
        count,
        counter,
        holder,
    } = removal;
    match holder {
        CounterHolderNode::Source(noun) => {
            cursor.enter("activation.remove-counters.source-noun");
            cursor.require(
                noun == PaymentSourceNoun::Permanent,
                "a permanent source noun",
            )?;
            // Several unnamed counters may be of several kinds: each removed
            // counter is chosen, so they are a distribution on the source.
            if counter.is_none() && count != NumericAtom::Integer(1) {
                let count = literal_counter_count(cursor, count)?;
                return Ok(LocalActivationCostNode::RemoveControlledCounters(
                    ControlledCounterRemovalCostNode {
                        holder: CounterDistributionHolder::Source,
                        counter,
                        count,
                    },
                ));
            }
            // Counters of one named kind on the source: a literal count or
            // the activation's chosen X (`Remove X +1/+1 counters`).
            Ok(LocalActivationCostNode::RemoveSourceCounters(
                SourceCounterRemovalCostNode {
                    counter,
                    count,
                    cost_index,
                },
            ))
        }
        CounterHolderNode::One(object) => {
            // `from a creature`: N counters on one permanent are not a
            // distribution among several; only one counter is equivalent.
            cursor.enter("activation.remove-counters.one-holder-quantity");
            cursor.require(
                count == NumericAtom::Integer(1),
                "one counter when removing from one chosen permanent",
            )?;
            controlled_counter_removal(cursor, object, counter, 1)
        }
        CounterHolderNode::Among(object) => {
            let count = literal_counter_count(cursor, count)?;
            controlled_counter_removal(cursor, object, counter, count)
        }
    }
}

/// True when a payment action's quantity slot is the variable X.
fn payment_quantity_uses_x(payment: &PaymentActionNode) -> bool {
    match payment {
        PaymentActionNode::Tap(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Untap(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Sacrifice(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Discard(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::ReturnToHand(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Exert(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Chosen(chosen),
            ..
        }
        | PaymentActionNode::Reveal {
            objects: PaymentObjectsNode::Chosen(chosen),
            ..
        } => chosen.count == NumericAtom::VariableX,
        PaymentActionNode::RemoveCounters(removal) => removal.count == NumericAtom::VariableX,
        PaymentActionNode::PutCounters(placement) => placement.count == NumericAtom::VariableX,
        _ => false,
    }
}

/// A declared object choice is enumerated with a literal cardinality: the
/// engine's `chooseObjects` declaration takes an exact literal quantity, so a
/// chosen-X number of objects is a typed engine gap, never approximated.
fn declared_object_count(
    cursor: &mut OracleTokenCursor<'_>,
    count: NumericAtom,
    state: &'static str,
) -> MachineResult<i64> {
    literal_quantity(count).ok_or_else(|| {
        cursor.enter(state);
        cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
            "a literal number of objects chosen while paying (a declaration of X chosen objects has no engine schema)",
        )
    })
}

/// Counter placement and distributed removal pay a literal number of counters.
fn literal_counter_count(
    cursor: &mut OracleTokenCursor<'_>,
    count: NumericAtom,
) -> MachineResult<i64> {
    literal_quantity(count).ok_or_else(|| {
        cursor.enter("activation.counters.variable-quantity");
        cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
            "a literal counter quantity for this counter cost (only removal of named counters from the source takes X)",
        )
    })
}

fn controlled_counter_removal(
    cursor: &mut OracleTokenCursor<'_>,
    object: ObjectQueryNode,
    counter: Option<CounterName>,
    count: i64,
) -> MachineResult<LocalActivationCostNode> {
    cursor.enter("activation.remove-counters.numeric-context");
    cursor.require(
        !object_predicate_uses_x(&object.predicate),
        "no {X} in the counter-holder predicate",
    )?;
    Ok(LocalActivationCostNode::RemoveControlledCounters(
        ControlledCounterRemovalCostNode {
            holder: CounterDistributionHolder::Permanents(object),
            counter,
            count,
        },
    ))
}

#[cfg(test)]
#[path = "activation_cost_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "activation_selected_sacrifice_tests.rs"]
mod selected_sacrifice_tests;
