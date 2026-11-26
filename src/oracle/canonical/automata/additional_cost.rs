//! Additional costs of a spell (CR 601.2b, 601.2f-h, 118.8).
//!
//! ```text
//! AdditionalCost := Header ["you" "may"] Alternatives "." [Reminder]
//! Header         := "as an additional cost to cast this spell ,"
//! Alternatives   := Branch
//!                 | Branch "or" Branch ("or" Branch)*
//!                 | Branch ("," Branch)+ "," "or" Branch
//! Branch         := PaymentAction ("and" PaymentAction)*
//! ```
//!
//! The header is the shared `recognize_mandatory_additional_cost_header`;
//! each payment action is the shared cost reader (`payment_action.rs`), so the
//! additional cost reuses every action, object query and card selector of
//! activation and resolution costs. This frame only owns the alternatives
//! (CR 601.2b: the caster announces which one they pay), the optional
//! envelope (`you may`, a choice whose payment the spell may later read) and
//! the projection of each action onto the casting cost schema. A branch or an
//! action the casting schema cannot represent fails explicitly at its own
//! state; an unsupported alternative never degrades to the other one.
use crate::oracle::counter_name::CounterName;
use crate::oracle::syntax::recognize_oracle_entities;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionSelectionMethod, CounterHolderNode, ManaCostNode, NumericAtom, ObjectControllerScope,
    ObjectKind, ObjectPredicate, ObjectQueryNode, PayLifeCostNode, PaymentActionNode,
    PaymentObjectFilter, PaymentObjectsNode, PaymentOriginZone, ResourcePaymentNode, number_word,
    object_predicate_uses_x, recognize_mandatory_additional_cost_header, recognize_numeric_atom,
    recognize_payment_action,
};

/// One action of an additional cost, projected onto the casting cost schema.
/// Every chosen object is chosen while casting (CR 601.2h), never targeted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AdditionalCostActionNode {
    /// `pay {2}{B}`.
    PayMana(ManaCostNode),
    /// `pay N life`.
    PayLife(PayLifeCostNode),
    /// `sacrifice N QUERY` (permanents the caster controls, CR 701.17a).
    Sacrifice { count: i64, object: ObjectQueryNode },
    /// `discard N CARDS` (cards in the caster's hand).
    Discard { count: i64, filter: ObjectPredicate },
    /// `exile N CARDS from your hand|graveyard`.
    ExileCards {
        count: i64,
        filter: ObjectPredicate,
        zone: PaymentOriginZone,
    },
    /// `tap N untapped QUERY you control`.
    Tap { count: i64, object: ObjectQueryNode },
    /// `return N QUERY you control to its owner's hand`.
    ReturnToHand { count: i64, object: ObjectQueryNode },
    /// `reveal N CARDS from your hand`.
    Reveal { count: i64, filter: ObjectPredicate },
    /// `put N KIND counters on a QUERY you control`.
    PutCounters {
        counter: CounterName,
        count: i64,
        object: ObjectQueryNode,
    },
    /// `blight N` (CR 701.65a): N -1/-1 counters on a creature the caster
    /// controls.
    Blight { amount: i64 },
}

impl AdditionalCostActionNode {
    /// The name of the casting mode that selects a branch starting with this
    /// action (CR 601.2b announces the chosen alternative).
    pub(in crate::oracle::canonical) fn mode_name(&self) -> &'static str {
        match self {
            Self::PayMana(_) => "payMana",
            Self::PayLife(_) => "payLife",
            Self::Sacrifice { .. } => "sacrifice",
            Self::Discard { .. } => "discard",
            Self::ExileCards { .. } => "exile",
            Self::Tap { .. } => "tap",
            Self::ReturnToHand { .. } => "return",
            Self::Reveal { .. } => "reveal",
            Self::PutCounters { .. } => "putCounters",
            Self::Blight { .. } => "blight",
        }
    }
}

/// The actions of one alternative, all paid (CR 118.3: no partial payment).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AdditionalCostBranchNode {
    pub actions: Vec<AdditionalCostActionNode>,
}

/// One additional-cost ability: the alternatives the caster chooses among
/// (exactly one branch when the cost has no `or`), optionally declined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AdditionalCostFrameNode {
    /// `you may`: the caster may decline the whole cost.
    pub optional: bool,
    pub branches: Vec<AdditionalCostBranchNode>,
}

/// Structural claim: the line starts with the mandatory additional-cost
/// header of this spell. Lines naming other spells (`to cast creature
/// spells`) are a static ability of another frame.
pub(in crate::oracle::canonical) fn additional_cost_frame_starts(
    tokens: &[crate::oracle::model::RecognizedEntity],
) -> bool {
    super::has_mandatory_additional_cost_header(tokens)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Separator {
    Or,
    Comma,
    CommaOr,
}

pub(in crate::oracle::canonical) fn recognize_additional_cost_frame(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AdditionalCostFrameNode> {
    cursor.enter("additional-cost.header");
    recognize_mandatory_additional_cost_header(cursor)?;
    cursor.enter("additional-cost.optional");
    let optional = word_at(cursor, 0, "you") && word_at(cursor, 1, "may");
    if optional {
        cursor.consume_phrase(&["you", "may"])?;
    }
    let mut branches = vec![recognize_branch(cursor)?];
    let mut separators = Vec::new();
    loop {
        cursor.enter("additional-cost.connector-or-terminator");
        let separator = if word_at(cursor, 0, ",") && word_at(cursor, 1, "or") {
            cursor.consume_phrase(&[",", "or"])?;
            Separator::CommaOr
        } else if word_at(cursor, 0, ",") {
            cursor.consume(",")?;
            Separator::Comma
        } else if word_at(cursor, 0, "or") {
            cursor.consume("or")?;
            Separator::Or
        } else {
            break;
        };
        separators.push(separator);
        branches.push(recognize_branch(cursor)?);
    }
    // `A, B, or C`: commas separate alternatives only in a list closed by
    // `, or`; `A or B or C` repeats the bare connector. A comma list that is
    // not closed, or mixes both forms, has no alternative reading.
    cursor.enter("additional-cost.alternatives.list");
    let commas = separators
        .iter()
        .filter(|separator| **separator != Separator::Or)
        .count();
    cursor.require(
        commas == 0
            || (separators.last() == Some(&Separator::CommaOr)
                && separators[..separators.len() - 1]
                    .iter()
                    .all(|separator| *separator == Separator::Comma)),
        "alternatives joined by `or`, or a comma list closed by `, or`",
    )?;
    cursor.enter("additional-cost.terminator");
    cursor.consume(".")?;
    let frame = AdditionalCostFrameNode { optional, branches };
    if word_at(cursor, 0, "(") {
        recognize_reminder(cursor, &frame)?;
    }
    cursor.enter("additional-cost.complete");
    cursor.finish()?;
    // The spell reads an optional cost as paid or not paid; it has no
    // reference naming which of several optional alternatives was paid.
    cursor.enter("additional-cost.optional.alternatives");
    cursor.require(
        !(frame.optional && frame.branches.len() > 1),
        "one optional additional cost (an optional choice among alternatives has no paid-branch reference)",
    )?;
    Ok(frame)
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `ACTION (and ACTION)*`: one alternative, paid entirely.
fn recognize_branch(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<AdditionalCostBranchNode> {
    let mut actions = Vec::new();
    loop {
        cursor.enter("additional-cost.action");
        let payment = recognize_payment_action(cursor)?;
        actions.push(project_payment_action(cursor, payment)?);
        if word_at(cursor, 0, "and") {
            cursor.enter("additional-cost.action.conjunction");
            cursor.consume("and")?;
            continue;
        }
        return Ok(AdditionalCostBranchNode { actions });
    }
}

/// A chosen object count: a literal. The casting declaration chooses an
/// exact literal number of objects; a chosen-X number of objects has no
/// casting schema yet and is an explicit engine gap.
fn literal_count(cursor: &mut OracleTokenCursor<'_>, count: NumericAtom) -> MachineResult<i64> {
    let count = literal_quantity(count).ok_or_else(|| {
        cursor.enter("additional-cost.action.quantity");
        cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::AdditionalCostAction,
            "a literal number of objects chosen while casting (a declaration of X chosen objects has no casting schema)",
        )
    })?;
    cursor.enter("additional-cost.action.quantity");
    cursor.require(
        count <= MAX_CHOSEN_COST_OBJECTS,
        "a chosen-object count within the bounded casting declaration (64)",
    )?;
    Ok(count)
}

/// The casting declaration enumerates each chosen object as its own payment.
const MAX_CHOSEN_COST_OBJECTS: i64 = 64;

/// The permanents an additional cost acts on are the caster's own: the query
/// names no controller, or names the caster (`you control`).
fn caster_permanents(
    cursor: &mut OracleTokenCursor<'_>,
    filter: PaymentObjectFilter,
) -> MachineResult<ObjectQueryNode> {
    cursor.enter("additional-cost.action.permanents");
    let PaymentObjectFilter::Permanents(object) = filter else {
        return Err(cursor.fail("permanents the caster controls"));
    };
    cursor.require(
        matches!(
            object.controller,
            None | Some(ObjectControllerScope::AbilityController)
        ) && !object.exclude_source,
        "permanents the caster controls",
    )?;
    cursor.require(
        object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "a permanent object predicate with a nominal head",
    )?;
    cursor.enter("additional-cost.action.numeric-context");
    cursor.require(
        !object_predicate_uses_x(&object.predicate),
        "no X in the chosen-permanent predicate",
    )?;
    Ok(object)
}

fn caster_cards(
    cursor: &mut OracleTokenCursor<'_>,
    filter: PaymentObjectFilter,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("additional-cost.action.cards");
    let PaymentObjectFilter::Cards(filter) = filter else {
        return Err(cursor.fail("cards in one of the caster's zones"));
    };
    cursor.require(
        filter.supports_domain(ObjectKind::Card),
        "a card-domain operand",
    )?;
    cursor.enter("additional-cost.action.numeric-context");
    cursor.require(
        !object_predicate_uses_x(&filter),
        "no X in the chosen-card predicate",
    )?;
    Ok(filter)
}

/// The objects a chosen-object action names: chosen by the caster while
/// casting, a literal number of them.
fn chosen_objects(
    cursor: &mut OracleTokenCursor<'_>,
    objects: PaymentObjectsNode,
) -> MachineResult<(i64, PaymentObjectFilter)> {
    match objects {
        PaymentObjectsNode::Chosen(chosen) => {
            cursor.enter("additional-cost.action.selection");
            cursor.require(
                !chosen.exclude_source,
                "chosen objects without `another` (a spell's cost has no source permanent to exclude)",
            )?;
            if chosen.method != CollectionSelectionMethod::Choice {
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::AdditionalCostAction,
                    "objects chosen by the caster (a random casting payment has no casting schema)",
                ));
            }
            // A number the caster chooses while casting (`exile any number
            // of black cards from your hand`) has no casting declaration.
            let Some(count) = chosen.count.exact() else {
                cursor.enter("additional-cost.action.chosen-quantity");
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::AdditionalCostAction,
                    "a stated number of objects chosen while casting (a number the caster chooses has no casting declaration)",
                ));
            };
            let count = literal_count(cursor, count)?;
            Ok((count, chosen.filter))
        }
        PaymentObjectsNode::Source(_) => {
            cursor.enter("additional-cost.action.source");
            Err(cursor.fail(
                "objects the caster chooses (a spell's additional cost does not act on the spell)",
            ))
        }
        PaymentObjectsNode::Designated(_) => {
            cursor.enter("additional-cost.action.designated");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::AdditionalCostAction,
                "objects the caster chooses (a whole hand, a zone's top card or an attached permanent has no casting schema)",
            ))
        }
    }
}

/// Projects one shared payment action onto the casting cost schema.
fn project_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    payment: PaymentActionNode,
) -> MachineResult<AdditionalCostActionNode> {
    cursor.enter("additional-cost.action.schema");
    match payment {
        PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana)) => {
            cursor.enter("additional-cost.action.pay-mana");
            cursor.require(
                !mana.contains_variable_x(),
                "a fixed mana additional cost (an {X} additional payment has no producer here)",
            )?;
            Ok(AdditionalCostActionNode::PayMana(mana))
        }
        PaymentActionNode::Pay(ResourcePaymentNode::Life(life)) => {
            Ok(AdditionalCostActionNode::PayLife(life))
        }
        PaymentActionNode::Sacrifice(objects) => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let object = caster_permanents(cursor, filter)?;
            Ok(AdditionalCostActionNode::Sacrifice { count, object })
        }
        PaymentActionNode::Discard(objects) => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let filter = caster_cards(cursor, filter)?;
            Ok(AdditionalCostActionNode::Discard { count, filter })
        }
        PaymentActionNode::Exile {
            objects,
            origin: Some(zone),
        } => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let filter = caster_cards(cursor, filter)?;
            Ok(AdditionalCostActionNode::ExileCards {
                count,
                filter,
                zone,
            })
        }
        PaymentActionNode::Tap(objects) => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let mut object = caster_permanents(cursor, filter)?;
            // Only an untapped permanent can be tapped (CR 701.26a): the
            // shared reader treats the written status as redundant, so the
            // casting choice states it to offer only payable permanents.
            object.predicate = ObjectPredicate::Intersection(vec![
                object.predicate,
                ObjectPredicate::Status(super::ObjectStatus::Untapped),
            ]);
            Ok(AdditionalCostActionNode::Tap { count, object })
        }
        PaymentActionNode::ReturnToHand(objects) => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let object = caster_permanents(cursor, filter)?;
            Ok(AdditionalCostActionNode::ReturnToHand { count, object })
        }
        PaymentActionNode::Reveal {
            objects,
            origin: Some(PaymentOriginZone::Hand),
        } => {
            let (count, filter) = chosen_objects(cursor, objects)?;
            let filter = caster_cards(cursor, filter)?;
            Ok(AdditionalCostActionNode::Reveal { count, filter })
        }
        PaymentActionNode::PutCounters(placement) => {
            cursor.enter("additional-cost.action.put-counters");
            let CounterHolderNode::One(object) = placement.holder else {
                return Err(cursor.fail("one permanent the caster controls holding the counters"));
            };
            let count = literal_count(cursor, placement.count)?;
            let object = caster_permanents(cursor, PaymentObjectFilter::Permanents(object))?;
            Ok(AdditionalCostActionNode::PutCounters {
                counter: placement.counter,
                count,
                object,
            })
        }
        PaymentActionNode::Blight { amount } => Ok(AdditionalCostActionNode::Blight { amount }),
        PaymentActionNode::Pay(ResourcePaymentNode::PlayerCounters(_))
        | PaymentActionNode::Exile { origin: None, .. }
        | PaymentActionNode::Reveal { .. }
        | PaymentActionNode::TapTotalPower(_)
        | PaymentActionNode::Untap(_)
        | PaymentActionNode::Exert(_)
        | PaymentActionNode::Mill { .. }
        | PaymentActionNode::PutInLibrary { .. }
        | PaymentActionNode::RemoveCounters(_) => {
            cursor.enter("additional-cost.action.unsupported");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::AdditionalCostAction,
                "an additional cost the casting schema can pay (mana, life, sacrifice, discard, exile from a card zone, tap, return, reveal from hand, counters on a permanent, blight)",
            ))
        }
    }
}

/// The printed reminder of a Blight cost restates the definition with the
/// same amount (CR 701.65a). Any other reminder is not this frame's.
fn recognize_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &AdditionalCostFrameNode,
) -> MachineResult<()> {
    cursor.enter("additional-cost.reminder");
    let amounts = frame
        .branches
        .iter()
        .flat_map(|branch| &branch.actions)
        .filter_map(|action| match action {
            AdditionalCostActionNode::Blight { amount } => Some(*amount),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [amount] = amounts.as_slice() else {
        return Err(cursor.fail("a reminder only after one Blight cost"));
    };
    let counters = match *amount {
        1 => "a -1/-1 counter".to_string(),
        amount => format!(
            "{} -1/-1 counters",
            number_word(amount).ok_or_else(|| cursor.fail("a Blight amount with a number word"))?
        ),
    };
    let renderings = [
        format!("(To blight {amount}, put {counters} on a creature you control.)"),
        format!("(You may put {counters} on a creature you control.)"),
    ];
    let printed = cursor.token_slice(cursor.position(), usize::MAX);
    let matched = renderings.iter().find_map(|rendering| {
        let expected = recognize_oracle_entities(rendering);
        (expected.len() <= printed.len()
            && expected
                .iter()
                .zip(printed)
                .all(|(expected, token)| token.raw.eq_ignore_ascii_case(&expected.raw)))
        .then_some(expected.len())
    });
    let length =
        cursor.require_value(matched, "the Blight reminder restating the printed amount")?;
    cursor.advance_to(cursor.position() + length)
}

pub(in crate::oracle::canonical) fn positive_literal(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &str,
) -> MachineResult<i64> {
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("a") || token.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
        return Ok(1);
    }
    let NumericAtom::Integer(amount) = recognize_numeric_atom(cursor)? else {
        return Err(cursor.fail(expected));
    };
    cursor.require(amount > 0, expected)?;
    Ok(amount)
}

/// The quantity slot of a counted cost action: `a`/`an` (one), a positive
/// literal, or the variable X (CR 107.3). The slot never binds X; the cost
/// parent validates that its own ability declares X before lowering.
pub(in crate::oracle::canonical) fn positive_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &str,
) -> MachineResult<NumericAtom> {
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("a") || token.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
        return Ok(NumericAtom::Integer(1));
    }
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(amount) => {
            cursor.require(amount > 0, expected)?;
            Ok(NumericAtom::Integer(amount))
        }
        NumericAtom::VariableX => Ok(NumericAtom::VariableX),
    }
}

/// Noun agreement of a quantity slot: only a literal one is singular.
pub(in crate::oracle::canonical) fn quantity_is_plural(count: NumericAtom) -> bool {
    count != NumericAtom::Integer(1)
}

/// The literal value of a quantity slot, when it has one.
pub(in crate::oracle::canonical) fn literal_quantity(count: NumericAtom) -> Option<i64> {
    match count {
        NumericAtom::Integer(value) => Some(value),
        NumericAtom::VariableX => None,
    }
}

#[cfg(test)]
#[path = "additional_cost_frame_tests.rs"]
mod frame_tests;
