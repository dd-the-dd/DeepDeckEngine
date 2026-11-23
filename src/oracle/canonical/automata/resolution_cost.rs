//! Optional resolution cost and its outcome continuation (CR 608.2c, 603.12):
//!
//! ```text
//! OptionalCost := CostSequence "." Outcome "," PROGRAM
//! Outcome      := "if you do" | "if you don't" | "when you do"
//! ```
//!
//! The cost is the shared `CostSequenceNode` of ordinary payment actions (see
//! `payment_action`). This parent projects each action onto the engine's
//! resolution `CostProgram` schema; an action without such a schema (several
//! chosen objects to sacrifice, discard or exile, a random discard, counters
//! of several unnamed kinds) fails explicitly at its own state. A list of
//! actions is one all-or-nothing `costSequence` (CR 118.3, 601.2h) whose parts
//! must not compete for battlefield resources. The outcome is program control
//! only: the body is an ordinary shared effect sequence chosen by the
//! enclosing program parent.
use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionSelectionMethod, CostListGrammar, CostSequenceNode, CounterHolderNode,
    ObjectPredicate, ObjectQueryNode, PaymentActionNode, PaymentObjectFilter, PaymentObjectsNode,
    PaymentOriginZone, PaymentSourceNoun, RemoveCountersPaymentNode, ResourcePaymentNode,
    TapObjectsCostNode, payment_action_starts, recognize_cost_sequence,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ResolutionCostNode {
    Payment(ResourcePaymentNode),
    /// One permanent the payer controls; `another` excludes the source object.
    Sacrifice {
        object: ObjectQueryNode,
        exclude_source: bool,
    },
    /// `sacrifice this PERMANENT`: the resolving ability's source.
    SacrificeSource,
    /// One card from the payer's hand matching the shared card selector.
    DiscardCard {
        filter: ObjectPredicate,
    },
    /// `blight N` (keyword action): N -1/-1 counters on one creature the
    /// payer controls. Impossible without a creature, so it gates outcomes.
    Blight {
        amount: i64,
    },
    /// Tap N untapped permanents the payer controls (CR 118.3).
    TapObjects(TapObjectsCostNode),
    /// Counters removed from the resolving ability's source. An unnamed kind
    /// is one counter of any kind the payer chooses.
    RemoveSourceCounters {
        counter: Option<CounterName>,
        count: i64,
    },
    /// One card chosen from the payer's hand or graveyard and exiled.
    ExileCard {
        filter: ObjectPredicate,
        zone: PaymentOriginZone,
        exclude_source: bool,
    },
    /// `A and B`: every part is paid, or none (CR 118.3).
    Sequence(Vec<ResolutionCostNode>),
}

/// The resource a resolution cost part consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CostResource {
    Hand,
    Graveyard,
    /// Mana, life and battlefield objects: mana abilities may consume any.
    Battlefield,
}

impl ResolutionCostNode {
    /// Two parts of one list must not compete for one resource (a land
    /// tapped for mana or sacrificed, one card exiled twice): then each
    /// part's own payability is the whole list's payability (CR 118.3).
    fn resource(&self) -> CostResource {
        match self {
            Self::DiscardCard { .. }
            | Self::ExileCard {
                zone: PaymentOriginZone::Hand,
                ..
            } => CostResource::Hand,
            Self::ExileCard {
                zone: PaymentOriginZone::Graveyard,
                ..
            } => CostResource::Graveyard,
            _ => CostResource::Battlefield,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostOutcomeContinuation {
    /// `If you do, …` — the body resolves only if the cost was paid.
    IfPaid,
    /// `If you don't, …` — the body resolves only if the cost was not paid.
    IfDeclined,
    /// `When you do, …` — a reflexive trigger with its own targets (CR 603.12).
    WhenPaid,
}

pub(in crate::oracle::canonical) fn optional_cost_head(cursor: &OracleTokenCursor<'_>) -> bool {
    !cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol" || token.token_type == "tapSymbol")
        && payment_action_starts(cursor)
}

/// True when the first sentence after the cursor is followed by a typed cost
/// outcome connector. Lexical lookahead only; it commits nothing.
pub(in crate::oracle::canonical) fn cost_outcome_follows_first_sentence(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "." {
            return outcome_at(cursor, offset + 1).is_some();
        }
        offset += 1;
    }
    false
}

pub(in crate::oracle::canonical) fn outcome_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(CostOutcomeContinuation, usize)> {
    let word = |index: usize| {
        cursor
            .peek_ahead(offset + index)
            .map(str::to_ascii_lowercase)
    };
    // The connector's subject is the permission's performer: `you`, or
    // another player named by `they` / `that player` / `the player`
    // (CR 608.2c `if [that player] does`).
    let (subject_width, singular) = match (word(1)?.as_str(), word(2).as_deref()) {
        ("you" | "they", _) => (1, false),
        ("that" | "the", Some("player")) => (2, true),
        _ => return None,
    };
    let verb = word(1 + subject_width)?;
    let outcome = match (word(0)?.as_str(), verb.as_str(), singular) {
        ("if", "do", false) | ("if", "does", true) => CostOutcomeContinuation::IfPaid,
        ("if", "don't" | "don’t", false) | ("if", "doesn't" | "doesn’t", true) => {
            CostOutcomeContinuation::IfDeclined
        }
        ("when", "do", false) | ("when", "does", true) => CostOutcomeContinuation::WhenPaid,
        _ => return None,
    };
    let width = 2 + subject_width;
    (word(width)?.as_str() == ",").then_some((outcome, width + 1))
}

/// One resolution cost: the shared cost sequence projected onto the engine's
/// single resolution `CostProgram`.
pub(in crate::oracle::canonical) fn recognize_resolution_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResolutionCostNode> {
    cursor.enter("cost.resolution.verb");
    let sequence = recognize_cost_sequence(
        cursor,
        CostListGrammar {
            comma: true,
            and: true,
        },
    )?;
    project_resolution_cost_sequence(cursor, sequence)
}

/// Projects a recognized cost sequence onto one resolution `CostProgram`,
/// whichever parent read it (an optional cost, an `unless` payment).
pub(in crate::oracle::canonical) fn project_resolution_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: CostSequenceNode<PaymentActionNode>,
) -> MachineResult<ResolutionCostNode> {
    cursor.enter("cost.resolution.sequence");
    let mut parts = sequence
        .costs
        .into_iter()
        .map(|payment| resolution_cost_from_payment(cursor, payment))
        .collect::<MachineResult<Vec<_>>>()?;
    if parts.len() == 1 {
        return Ok(parts.remove(0));
    }
    cursor.enter("cost.resolution.sequence.resources");
    let mut resources = parts
        .iter()
        .map(ResolutionCostNode::resource)
        .collect::<Vec<_>>();
    resources.sort();
    cursor.require(
        resources.windows(2).all(|pair| pair[0] != pair[1]),
        "resolution cost parts drawing on distinct resources (one mana, life or battlefield payment; one card per zone)",
    )?;
    Ok(ResolutionCostNode::Sequence(parts))
}

/// Projects one payment action onto the resolution `CostProgram` schema
/// (`src/engine/resolution_costs`). The payer is the enclosing program's.
pub(in crate::oracle::canonical) fn resolution_cost_from_payment(
    cursor: &mut OracleTokenCursor<'_>,
    payment: PaymentActionNode,
) -> MachineResult<ResolutionCostNode> {
    match payment {
        // Actions the resolution cost program has no schema for fail here,
        // never approximated by another program.
        PaymentActionNode::Untap(_)
        | PaymentActionNode::ReturnToHand(_)
        | PaymentActionNode::PutCounters(_)
        | PaymentActionNode::Reveal { .. }
        | PaymentActionNode::Mill { .. }
        | PaymentActionNode::Exert(_) => {
            cursor.enter("cost.resolution.unsupported-action");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ResolutionCostAction,
                "a resolution cost the engine can pay (pay, sacrifice, discard, tap, exile, remove counters, blight)",
            ))
        }
        PaymentActionNode::Pay(payment) => Ok(ResolutionCostNode::Payment(payment)),
        PaymentActionNode::Blight { amount } => Ok(ResolutionCostNode::Blight { amount }),
        PaymentActionNode::Sacrifice(PaymentObjectsNode::Source(noun)) => {
            cursor.enter("cost.resolution.sacrifice.source");
            cursor.require(
                matches!(
                    noun,
                    PaymentSourceNoun::PermanentType(_) | PaymentSourceNoun::Token
                ),
                "a permanent source noun",
            )?;
            Ok(ResolutionCostNode::SacrificeSource)
        }
        PaymentActionNode::Sacrifice(PaymentObjectsNode::Chosen(chosen)) => {
            cursor.enter("cost.resolution.sacrifice.quantity");
            cursor.require(chosen.count == 1, "one sacrificed permanent")?;
            let PaymentObjectFilter::Permanents(object) = chosen.filter else {
                return Err(cursor.fail("a permanent operand"));
            };
            Ok(ResolutionCostNode::Sacrifice {
                object,
                exclude_source: chosen.exclude_source,
            })
        }
        PaymentActionNode::Discard(PaymentObjectsNode::Chosen(chosen)) => {
            cursor.enter("cost.resolution.discard.quantity");
            cursor.require(chosen.count == 1, "one discarded card")?;
            cursor.enter("cost.resolution.discard.source-exclusion");
            cursor.require(!chosen.exclude_source, "a discarded card without `another`")?;
            cursor.enter("cost.resolution.discard.selection-method");
            cursor.require(
                chosen.method == CollectionSelectionMethod::Choice,
                "a payer-chosen discarded card (random cost discard is unsupported)",
            )?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            Ok(ResolutionCostNode::DiscardCard { filter })
        }
        PaymentActionNode::Discard(PaymentObjectsNode::Source(_)) => {
            cursor.enter("cost.resolution.discard.source");
            Err(cursor.fail("a discarded card from hand chosen while paying"))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Chosen(chosen)) => {
            let PaymentObjectFilter::Permanents(object) = chosen.filter else {
                return Err(cursor.fail("a permanent operand"));
            };
            Ok(ResolutionCostNode::TapObjects(TapObjectsCostNode {
                count: chosen.count,
                object,
                exclude_source: chosen.exclude_source,
            }))
        }
        PaymentActionNode::Tap(PaymentObjectsNode::Source(_)) => {
            cursor.enter("cost.resolution.tap.source");
            Err(cursor.fail("untapped permanents chosen while paying"))
        }
        PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Chosen(chosen),
            origin,
        } => {
            cursor.enter("cost.resolution.exile.quantity");
            cursor.require(chosen.count == 1, "one exiled card")?;
            cursor.enter("cost.resolution.exile.origin");
            let zone = cursor.require_value(origin, "`from your hand` or `from your graveyard`")?;
            cursor.enter("cost.resolution.exile.selection-method");
            cursor.require(
                chosen.method == CollectionSelectionMethod::Choice,
                "a payer-chosen exiled card (random cost exile is unsupported)",
            )?;
            let PaymentObjectFilter::Cards(filter) = chosen.filter else {
                return Err(cursor.fail("a card operand"));
            };
            Ok(ResolutionCostNode::ExileCard {
                filter,
                zone,
                exclude_source: chosen.exclude_source,
            })
        }
        PaymentActionNode::Exile {
            objects: PaymentObjectsNode::Source(_),
            ..
        } => {
            cursor.enter("cost.resolution.exile.source");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::ResolutionCostAction,
                "a resolution cost the engine can pay (exiling the source has no resolution cost program)",
            ))
        }
        PaymentActionNode::RemoveCounters(RemoveCountersPaymentNode {
            count,
            counter,
            holder,
        }) => {
            cursor.enter("cost.resolution.remove-counters.holder");
            cursor.require(
                matches!(
                    holder,
                    CounterHolderNode::Source(
                        PaymentSourceNoun::PermanentType(_) | PaymentSourceNoun::Token
                    )
                ),
                "counters removed from the source permanent (`from this PERMANENT`)",
            )?;
            cursor.enter("cost.resolution.remove-counters.unnamed-quantity");
            cursor.require(
                counter.is_some() || count == 1,
                "a named counter kind when removing more than one counter",
            )?;
            Ok(ResolutionCostNode::RemoveSourceCounters { counter, count })
        }
    }
}

/// Reads `. Outcome ,` after a complete cost.
pub(in crate::oracle::canonical) fn recognize_cost_outcome_continuation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CostOutcomeContinuation> {
    cursor.enter("cost.outcome.sentence-end");
    cursor.consume(".")?;
    recognize_outcome_connector(cursor)
}

/// Reads `If you do ,` / `If you don't ,` / `When you do ,` at a sentence
/// start. The outcome refers to the preceding cost or instruction.
pub(in crate::oracle::canonical) fn recognize_outcome_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CostOutcomeContinuation> {
    cursor.enter("cost.outcome.connector");
    let (outcome, width) = outcome_at(cursor, 0)
        .ok_or_else(|| cursor.fail("`If you do,`, `If you don't,` or `When you do,`"))?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(outcome)
}

#[cfg(test)]
#[path = "resolution_cost_tests.rs"]
mod tests;
