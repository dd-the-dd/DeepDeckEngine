//! Activation costs (CR 602.1a, 602.2b): the shared `CostSequenceNode` of
//! ordinary payment actions, projected onto the activation cost schema
//! (`costs[]` plus declared payment choices). The projection owns the
//! activation semantics: the source zone implied by `this card` actions, the
//! ordinal of each declared choice and the X producer. An action without an
//! activation representation fails here at its own state.
//!
//! Quantity slots are the shared numeric atom. X in an activation cost is the
//! value the activating player announces (CR 107.3a, 601.2b via 602.2b): the
//! activation itself is its producer (`xValue`), whether or not an `{X}` mana
//! symbol also pays it (`{T}, Sacrifice X Goats`). Other cost parents
//! (resolution, `unless`) have no announcement and fail binding instead.
use crate::oracle::counter_name::CounterName;
use crate::oracle::syntax::ResolvedNameLink;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ChosenAmountBounds, CollectionSelectionMethod, CostAmountVerb, CostChosenAmountSlot,
    CostConnector, CostListGrammar, CostQuantity, CounterHolderNode, DesignatedPaymentObjects,
    DiscardSelfCostNode, ExileSelfCostNode, ExileSelfSourceZone, ManaCostNode, NumericAtom,
    ObjectPredicate, ObjectQueryNode, PayLifeCostNode, PaymentActionNode, PaymentLibraryEnd,
    PaymentObjectFilter, PaymentObjectsNode, PaymentOriginZone, PaymentSourceNoun,
    PlayerCounterPaymentNode, RemoveCountersPaymentNode, ResourcePaymentNode,
    SacrificeSelfCostNode, TapObjectsCostNode, TapSelfCostNode, literal_quantity,
    object_predicate_uses_x, recognize_cost_sequence,
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
    /// `tap any number of QUERY with total power N or greater` (Crew).
    TapTotalPower(ChosenTotalPowerTapCostNode),
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
    /// `reveal this card from your hand` (CR 701.20a): the source card,
    /// activated from its owner's hand.
    RevealSelf,
    /// `reveal N CARDS from your hand`, chosen while paying.
    RevealCards(ChosenRevealCostNode),
    /// `mill N cards` (CR 701.13a): the payer's top N library cards.
    Mill {
        count: i64,
    },
    /// `exert this PERMANENT` (CR 701.43a): it won't untap during its
    /// controller's next untap step.
    ExertSelf,
    /// `sacrifice|tap|return|discard|exile|reveal X OBJECTS`: exactly the
    /// activation's announced X objects, chosen while paying.
    ChosenX(ChosenXCostNode),
    /// `blight N` (CR 701.65a): N -1/-1 counters on one creature the payer
    /// controls, chosen while paying.
    Blight(BlightActivationCostNode),
    /// An action on objects the cost names by place, never chosen.
    Designated(DesignatedActivationCost),
    /// `put N CARDS from your hand|graveyard on top of|on the bottom of
    /// your library` (CR 401.4): cards chosen while paying.
    PutCardsInLibrary(ChosenLibraryCostNode),
    /// `put this card from your graveyard on top of|on the bottom of your
    /// library`: the source card, activated from its owner's graveyard.
    PutSourceInLibrary(PaymentLibraryEnd),
}

/// N cards chosen from the payer's hand or graveyard and put on top of or on
/// the bottom of their owner's library as a cost (CR 401.4, 118.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenLibraryCostNode {
    pub count: i64,
    pub filter: ObjectPredicate,
    pub zone: PaymentOriginZone,
    pub position: PaymentLibraryEnd,
    pub cost_index: usize,
}

/// A cost whose objects the text fixes by place (CR 118.3: all of them must
/// be there to pay it): the payer's whole hand, the payer's top library
/// cards, the payer's top matching graveyard card, or the permanent the
/// source is attached to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DesignatedActivationCost {
    /// `discard your hand` (CR 701.9a); an empty hand pays it.
    DiscardHand,
    /// `exile the top N card(s) of your library`.
    ExileLibraryTop { count: i64 },
    /// `exile the top [PREDICATE] card of your graveyard`.
    ExileGraveyardTop { filter: ObjectPredicate },
    /// `tap enchanted|equipped NOUN`.
    TapAttached,
    /// `sacrifice enchanted|equipped NOUN`.
    SacrificeAttached,
}

/// What a chosen-X cost does to each chosen object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenXAction {
    Sacrifice,
    Tap,
    Return,
    Discard,
    Exile,
    Reveal,
}

impl ChosenXAction {
    pub(in crate::oracle::canonical) fn as_str(self) -> &'static str {
        match self {
            Self::Sacrifice => "sacrifice",
            Self::Tap => "tap",
            Self::Return => "returnToOwnersHand",
            Self::Discard => "discard",
            Self::Exile => "exile",
            Self::Reveal => "reveal",
        }
    }
}

/// How many objects a chosen-objects cost acts on: the value the activating
/// player announces as X (CR 107.3a, 601.2b via 602.2b), or a number the
/// payer chooses while paying (`any number of`, `one or more`, `up to N`,
/// CR 601.2h), declared as the activation's `costChosenAmount`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenObjectsAmount {
    AnnouncedX,
    PayerChosen(ChosenAmountBounds),
}

impl ChosenObjectsAmount {
    /// The amount of a counted chosen-objects cost, when it is not a literal.
    fn of(count: CostQuantity) -> Option<Self> {
        match count {
            CostQuantity::Exactly(NumericAtom::VariableX) => Some(Self::AnnouncedX),
            CostQuantity::Chosen(bounds) => Some(Self::PayerChosen(bounds)),
            CostQuantity::Exactly(NumericAtom::Integer(_)) => None,
        }
    }

    /// The declared number the choice's quantity reads.
    pub(in crate::oracle::canonical) fn decision_id(self) -> &'static str {
        match self {
            Self::AnnouncedX => "xValue",
            Self::PayerChosen(_) => super::COST_CHOSEN_AMOUNT,
        }
    }
}

/// X objects chosen while paying, X being the value the activating player
/// announces (CR 107.3a, 601.2b via 602.2b) or a number the payer chooses
/// while paying (CR 601.2h): one declared choice of exactly that many
/// objects, paid by one cost over the whole choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenXCostNode {
    pub action: ChosenXAction,
    pub amount: ChosenObjectsAmount,
    pub filter: PaymentObjectFilter,
    /// The card zone the chosen cards are in (hand or graveyard); `None`
    /// for permanents.
    pub zone: Option<PaymentOriginZone>,
    pub exclude_source: bool,
    pub cost_index: usize,
}

/// N cards of the payer's hand revealed as a cost, chosen while paying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenRevealCostNode {
    pub count: i64,
    pub filter: ObjectPredicate,
    pub cost_index: usize,
}

/// The creature a Blight cost puts its counters on is chosen while paying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BlightActivationCostNode {
    pub amount: i64,
    pub cost_index: usize,
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
/// count is a literal, this activation's announced X, or a number of
/// counters of a named kind the payer chooses while paying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCounterRemovalCostNode {
    pub counter: Option<CounterName>,
    pub count: CostQuantity,
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

/// Untapped permanents chosen while paying whose total power is at least
/// the cost's bound (CR 702.122a). The ordinal names the declared choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenTotalPowerTapCostNode {
    pub tap: super::TotalPowerTapCostNode,
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
    /// The activation announces X (CR 107.3a, 601.2b via 602.2b) when a cost
    /// names it: an `{X}` mana symbol or an X quantity slot.
    pub(in crate::oracle::canonical) fn declares_x(&self) -> bool {
        self.costs.iter().any(|cost| match cost {
            LocalActivationCostNode::PayMana(mana) => mana.contains_variable_x(),
            LocalActivationCostNode::RemoveSourceCounters(removal) => {
                removal.count == NumericAtom::VariableX
            }
            LocalActivationCostNode::ChosenX(chosen) => {
                chosen.amount == ChosenObjectsAmount::AnnouncedX
            }
            _ => false,
        })
    }

    /// The bounds of the number the payer chooses while paying, if a cost
    /// of this activation has one (CR 601.2h via 602.2b). The cost sequence
    /// admits at most one such cost.
    pub(in crate::oracle::canonical) fn cost_chosen_amount(&self) -> Option<ChosenAmountBounds> {
        self.costs.iter().find_map(|cost| match cost {
            LocalActivationCostNode::RemoveSourceCounters(removal) => removal.count.chosen(),
            LocalActivationCostNode::ChosenX(ChosenXCostNode {
                amount: ChosenObjectsAmount::PayerChosen(bounds),
                ..
            }) => Some(*bounds),
            _ => None,
        })
    }

    /// What a later `for each counter removed this way`, `the number of
    /// cards exiled this way` or `that many` restates: the verb and the
    /// noun of the cost whose number the payer chose.
    pub(in crate::oracle::canonical) fn cost_chosen_amount_slot(
        &self,
    ) -> Option<CostChosenAmountSlot> {
        self.costs.iter().find_map(|cost| match cost {
            LocalActivationCostNode::RemoveSourceCounters(removal)
                if removal.count.chosen().is_some() =>
            {
                Some(CostChosenAmountSlot {
                    verb: CostAmountVerb::Removed,
                    counter: removal.counter.clone(),
                    predicate: None,
                })
            }
            LocalActivationCostNode::ChosenX(ChosenXCostNode {
                amount: ChosenObjectsAmount::PayerChosen(_),
                action,
                filter,
                ..
            }) => Some(CostChosenAmountSlot {
                verb: match action {
                    ChosenXAction::Sacrifice => CostAmountVerb::Sacrificed,
                    ChosenXAction::Tap => CostAmountVerb::Tapped,
                    ChosenXAction::Return => CostAmountVerb::Returned,
                    ChosenXAction::Discard => CostAmountVerb::Discarded,
                    ChosenXAction::Exile => CostAmountVerb::Exiled,
                    ChosenXAction::Reveal => CostAmountVerb::Revealed,
                },
                counter: None,
                predicate: Some(match filter {
                    PaymentObjectFilter::Permanents(object) => object.predicate.clone(),
                    PaymentObjectFilter::Cards(predicate) => predicate.clone(),
                }),
            }),
            _ => None,
        })
    }

    pub(in crate::oracle::canonical) fn source_zone_requirements(
        &self,
    ) -> ActivationSourceZoneRequirements {
        let mut requirements = ActivationSourceZoneRequirements::default();
        for cost in &self.costs {
            match cost {
                LocalActivationCostNode::DiscardSelf(_) | LocalActivationCostNode::RevealSelf => {
                    requirements.hand = true
                }
                LocalActivationCostNode::ExileSelf(exile) => match exile.source_zone {
                    ExileSelfSourceZone::Graveyard => requirements.graveyard = true,
                    ExileSelfSourceZone::Battlefield => requirements.battlefield = true,
                },
                LocalActivationCostNode::PutSourceInLibrary(_) => requirements.graveyard = true,
                LocalActivationCostNode::TapSelf(_)
                | LocalActivationCostNode::ExertSelf
                | LocalActivationCostNode::SacrificeSelf(_)
                | LocalActivationCostNode::ReturnSelf
                | LocalActivationCostNode::PutSourceCounters(_)
                | LocalActivationCostNode::RemoveSourceCounters(_)
                | LocalActivationCostNode::RemoveControlledCounters(
                    ControlledCounterRemovalCostNode {
                        holder: CounterDistributionHolder::Source,
                        ..
                    },
                )
                // An attached source is a permanent on the battlefield.
                | LocalActivationCostNode::Designated(
                    DesignatedActivationCost::TapAttached
                    | DesignatedActivationCost::SacrificeAttached,
                ) => requirements.battlefield = true,
                LocalActivationCostNode::PayMana(_)
                | LocalActivationCostNode::PayLife(_)
                | LocalActivationCostNode::PayPlayerCounters(_)
                | LocalActivationCostNode::DiscardCard(_)
                | LocalActivationCostNode::TapObjects(_)
                | LocalActivationCostNode::TapTotalPower(_)
                | LocalActivationCostNode::SacrificeObject(_)
                | LocalActivationCostNode::ReturnObjects(_)
                | LocalActivationCostNode::PutChosenCounters(_)
                | LocalActivationCostNode::RemoveControlledCounters(_)
                | LocalActivationCostNode::ExileCards(_)
                | LocalActivationCostNode::RevealCards(_)
                | LocalActivationCostNode::ChosenX(_)
                | LocalActivationCostNode::Mill { .. }
                | LocalActivationCostNode::PutCardsInLibrary(_)
                | LocalActivationCostNode::Blight(_)
                | LocalActivationCostNode::Designated(
                    DesignatedActivationCost::DiscardHand
                    | DesignatedActivationCost::ExileLibraryTop { .. }
                    | DesignatedActivationCost::ExileGraveyardTop { .. },
                ) => {}
            }
        }
        requirements
    }
}

/// The activation cost list of an ability whose name links are known: the
/// cost children may name the source by the card's name (`Sacrifice Major
/// Teroh:`, CR 201.5). The links are installed for the list only and the
/// enclosing ones restored, whatever the outcome.
pub(in crate::oracle::canonical) fn recognize_named_activation_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
) -> MachineResult<ActivationCostSequenceNode> {
    let previous = cursor.replace_source_names(Some(name_links.into()));
    let costs = recognize_activation_cost_sequence(cursor);
    cursor.replace_source_names(previous);
    costs
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
    let costs = sequence
        .costs
        .into_iter()
        .enumerate()
        .map(|(cost_index, payment)| activation_cost_from_payment(cursor, payment, cost_index))
        .collect::<MachineResult<Vec<_>>>()?;
    let costs = ActivationCostSequenceNode { costs, connectors };
    // One declared `costChosenAmount` per activation: two payer-chosen
    // numbers would need two producers a restatement could not tell apart.
    cursor.enter("activation.cost-chosen-amount.single");
    cursor.require(
        costs
            .costs
            .iter()
            .filter(|cost| match cost {
                LocalActivationCostNode::RemoveSourceCounters(removal) => {
                    removal.count.chosen().is_some()
                }
                LocalActivationCostNode::ChosenX(chosen) => {
                    chosen.amount != ChosenObjectsAmount::AnnouncedX
                }
                _ => false,
            })
            .count()
            <= 1,
        "at most one cost whose number the payer chooses while paying",
    )?;
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
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count) {
                return Ok(chosen_x_cost(
                    ChosenXAction::Tap,
                    amount,
                    PaymentObjectFilter::Permanents(object),
                    None,
                    chosen.exclude_source,
                    cost_index,
                ));
            }
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
        PaymentActionNode::TapTotalPower(tap) => {
            cursor.enter("activation.tap-total-power.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&tap.object.predicate),
                "no {X} in the tapped-permanent predicate",
            )?;
            Ok(LocalActivationCostNode::TapTotalPower(
                ChosenTotalPowerTapCostNode { tap, cost_index },
            ))
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
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count) {
                cursor.enter("activation.selected-sacrifice.numeric-context");
                cursor.require(
                    !object_predicate_uses_x(&object.predicate),
                    "no X in the predicate of X sacrificed permanents",
                )?;
                return Ok(chosen_x_cost(
                    ChosenXAction::Sacrifice,
                    amount,
                    PaymentObjectFilter::Permanents(object),
                    None,
                    chosen.exclude_source,
                    cost_index,
                ));
            }
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
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count)
                && chosen.method == CollectionSelectionMethod::Choice
            {
                return Ok(chosen_x_cost(
                    ChosenXAction::Discard,
                    amount,
                    PaymentObjectFilter::Cards(filter),
                    Some(PaymentOriginZone::Hand),
                    false,
                    cost_index,
                ));
            }
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
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count) {
                return Ok(chosen_x_cost(
                    ChosenXAction::Exile,
                    amount,
                    PaymentObjectFilter::Cards(filter),
                    Some(zone),
                    chosen.exclude_source,
                    cost_index,
                ));
            }
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
        // The engine pays Blight as counters put on one chosen creature
        // the payer controls (the shared `putCounters` cost).
        PaymentActionNode::Blight { amount } => {
            Ok(LocalActivationCostNode::Blight(BlightActivationCostNode {
                amount,
                cost_index,
            }))
        }
        // Objects named by place: each verb has the one projection its
        // engine cost pays; any other pairing fails explicitly.
        PaymentActionNode::Discard(PaymentObjectsNode::Designated(
            DesignatedPaymentObjects::Hand,
        )) => Ok(LocalActivationCostNode::Designated(
            DesignatedActivationCost::DiscardHand,
        )),
        PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Designated(DesignatedPaymentObjects::LibraryTop { count }),
            ..
        } => Ok(LocalActivationCostNode::Designated(
            DesignatedActivationCost::ExileLibraryTop { count },
        )),
        PaymentActionNode::Exile {
            objects:
                PaymentObjectsNode::Designated(DesignatedPaymentObjects::GraveyardTop { filter }),
            ..
        } => {
            cursor.enter("activation.exile-graveyard-top.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&filter),
                "no {X} in the predicate of the exiled top graveyard card",
            )?;
            Ok(LocalActivationCostNode::Designated(
                DesignatedActivationCost::ExileGraveyardTop { filter },
            ))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Designated(
            DesignatedPaymentObjects::Attached(_),
        )) => Ok(LocalActivationCostNode::Designated(
            DesignatedActivationCost::TapAttached,
        )),
        PaymentActionNode::Sacrifice(PaymentObjectsNode::Designated(
            DesignatedPaymentObjects::Attached(_),
        )) => Ok(LocalActivationCostNode::Designated(
            DesignatedActivationCost::SacrificeAttached,
        )),
        PaymentActionNode::Tap(PaymentObjectsNode::Designated(_))
        | PaymentActionNode::Sacrifice(PaymentObjectsNode::Designated(_))
        | PaymentActionNode::Discard(PaymentObjectsNode::Designated(_))
        | PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Designated(_),
            ..
        }
        | PaymentActionNode::ReturnToHand(PaymentObjectsNode::Designated(_))
        | PaymentActionNode::Reveal {
            objects: PaymentObjectsNode::Designated(_),
            ..
        } => {
            cursor.enter("activation.designated-objects.action");
            Err(cursor.fail(
                "a designated cost object its verb acts on (discard your hand, exile a zone's top card, tap or sacrifice the attached permanent)",
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
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count) {
                return Ok(chosen_x_cost(
                    ChosenXAction::Return,
                    amount,
                    PaymentObjectFilter::Permanents(object),
                    None,
                    chosen.exclude_source,
                    cost_index,
                ));
            }
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
        // `reveal this card from your hand`: the source card in its owner's
        // hand (CR 701.20a); a source permanent cannot be revealed.
        PaymentActionNode::Reveal {
            objects: PaymentObjectsNode::Source(noun),
            origin,
        } => {
            cursor.enter("activation.reveal-self.origin");
            cursor.require(
                noun == PaymentSourceNoun::Card && origin == Some(PaymentOriginZone::Hand),
                "`this card from your hand` for a revealed source",
            )?;
            Ok(LocalActivationCostNode::RevealSelf)
        }
        PaymentActionNode::Reveal {
            objects: PaymentObjectsNode::Chosen(chosen),
            origin,
        } => {
            cursor.enter("activation.reveal-cards.origin");
            cursor.require(
                origin == Some(PaymentOriginZone::Hand),
                "`from your hand` after the revealed cards",
            )?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            cursor.enter("activation.reveal-cards.selection");
            cursor.require(
                !chosen.exclude_source && chosen.method == CollectionSelectionMethod::Choice,
                "revealed cards of the payer's choice, without `another`",
            )?;
            cursor.enter("activation.reveal-cards.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&filter),
                "no {X} in the revealed-card predicate",
            )?;
            if let Some(amount) = ChosenObjectsAmount::of(chosen.count) {
                return Ok(chosen_x_cost(
                    ChosenXAction::Reveal,
                    amount,
                    PaymentObjectFilter::Cards(filter),
                    Some(PaymentOriginZone::Hand),
                    false,
                    cost_index,
                ));
            }
            let count = declared_object_count(cursor, chosen.count, "activation.reveal-cards")?;
            Ok(LocalActivationCostNode::RevealCards(ChosenRevealCostNode {
                count,
                filter,
                cost_index,
            }))
        }
        PaymentActionNode::Mill { count } => Ok(LocalActivationCostNode::Mill { count }),
        // `put this card from your graveyard on the bottom of your library`:
        // the source card leaves the graveyard the ability is activated
        // from. A source permanent put into a library has no activation
        // cost program (its `{T}` would be paid after it left).
        PaymentActionNode::PutInLibrary {
            objects: PaymentObjectsNode::Source(noun),
            origin,
            position,
        } => {
            cursor.enter("activation.put-self.origin");
            if noun == PaymentSourceNoun::Card && origin == Some(PaymentOriginZone::Graveyard) {
                return Ok(LocalActivationCostNode::PutSourceInLibrary(position));
            }
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
                "an activation cost the engine can pay (only `this card from your graveyard` is put into a library as a cost)",
            ))
        }
        PaymentActionNode::PutInLibrary {
            objects: PaymentObjectsNode::Chosen(chosen),
            origin,
            position,
        } => {
            cursor.enter("activation.put-cards.origin");
            let zone = cursor.require_value(origin, "`from your hand` or `from your graveyard`")?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            cursor.enter("activation.put-cards.selection");
            cursor.require(
                !chosen.exclude_source && chosen.method == CollectionSelectionMethod::Choice,
                "cards of the payer's choice, without `another`, put into a library",
            )?;
            cursor.enter("activation.put-cards.numeric-context");
            cursor.require(
                !object_predicate_uses_x(&filter),
                "no {X} in the predicate of the cards put into a library",
            )?;
            let count = declared_object_count(cursor, chosen.count, "activation.put-cards")?;
            Ok(LocalActivationCostNode::PutCardsInLibrary(
                ChosenLibraryCostNode {
                    count,
                    filter,
                    zone,
                    position,
                    cost_index,
                },
            ))
        }
        PaymentActionNode::PutInLibrary {
            objects: PaymentObjectsNode::Designated(_),
            ..
        } => {
            cursor.enter("activation.put-cards.designated");
            Err(cursor.fail("chosen cards or this card put into a library"))
        }
        PaymentActionNode::Exert(PaymentObjectsNode::Source(noun)) => {
            cursor.enter("activation.exert-self.source-type");
            cursor.require(
                noun == PaymentSourceNoun::Permanent,
                "a source noun naming a permanent after `exert`",
            )?;
            Ok(LocalActivationCostNode::ExertSelf)
        }
        // The engine has no activation-cost program for these actions; the
        // cost fails here, explicitly, instead of being approximated.
        PaymentActionNode::Untap(_) | PaymentActionNode::Exert(_) => {
            cursor.enter("activation.unsupported-action");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
                "an activation cost the engine can pay (untap and exerting chosen permanents have no activation cost schema)",
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
            // A payer-chosen number of counters (`remove any number of
            // storage counters`) is counted in one named kind: unnamed
            // counters of several kinds would be a chosen distribution.
            if count.chosen().is_some() {
                cursor.enter("activation.remove-counters.chosen-kind");
                cursor.require(
                    counter.is_some(),
                    "a named counter kind for a number of counters chosen while paying",
                )?;
            }
            // Several unnamed counters may be of several kinds: each removed
            // counter is chosen, so they are a distribution on the source.
            if counter.is_none() && count != NumericAtom::Integer(1) {
                let count = literal_cost_counter_count(cursor, count)?;
                return Ok(LocalActivationCostNode::RemoveControlledCounters(
                    ControlledCounterRemovalCostNode {
                        holder: CounterDistributionHolder::Source,
                        counter,
                        count,
                    },
                ));
            }
            // Counters of one named kind on the source: a literal count, the
            // activation's announced X (`Remove X +1/+1 counters`) or the
            // number the payer chooses (`Remove any number of ...`).
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
            let count = literal_cost_counter_count(cursor, count)?;
            controlled_counter_removal(cursor, object, counter, count)
        }
    }
}

fn chosen_x_cost(
    action: ChosenXAction,
    amount: ChosenObjectsAmount,
    filter: PaymentObjectFilter,
    zone: Option<PaymentOriginZone>,
    exclude_source: bool,
    cost_index: usize,
) -> LocalActivationCostNode {
    LocalActivationCostNode::ChosenX(ChosenXCostNode {
        action,
        amount,
        filter,
        zone,
        exclude_source,
        cost_index,
    })
}

/// A declared object choice of the schemas that take one choice per object
/// is enumerated with a literal cardinality; X or a payer-chosen number is
/// read before by the chosen-objects schema, and any other quantity (a
/// random discard of a chosen number) is a typed engine gap, never
/// approximated.
fn declared_object_count(
    cursor: &mut OracleTokenCursor<'_>,
    count: CostQuantity,
    state: &'static str,
) -> MachineResult<i64> {
    count.literal().ok_or_else(|| {
        cursor.enter(state);
        cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
            "a literal number of objects chosen while paying (a declaration of X chosen objects has no engine schema)",
        )
    })
}

/// Distributed removal pays a literal number of counters: a payer-chosen
/// number is counted only in one named kind on the source.
fn literal_cost_counter_count(
    cursor: &mut OracleTokenCursor<'_>,
    count: CostQuantity,
) -> MachineResult<i64> {
    match count {
        CostQuantity::Exactly(atom) => literal_counter_count(cursor, atom),
        CostQuantity::Chosen(_) => {
            cursor.enter("activation.counters.chosen-distribution");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ActivationCostAction,
                "a literal number of counters distributed while paying (a payer-chosen distribution has no engine schema)",
            ))
        }
    }
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
