use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterChangeDirection, EffectRecognitionContext, NumericAtom, NumericExpression,
    PutCountersOnObjectEffectNode, recognize_counter_name, recognize_numeric_atom,
    recognize_source_cast_x_numeric_expression, recognize_that_many_event_value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PutCountersState {
    Action,
    Count,
    CounterKind,
    CounterNoun,
    Relation,
    Permanent,
    Complete,
}

#[derive(Default)]
struct PutCountersMemory {
    permanent: Option<super::PermanentOperandNode>,
    counter: Option<crate::oracle::counter_name::CounterName>,
    count: Option<NumericExpression>,
}

/// A structural pre-claim that commits only after `put`, a plausible numeric
/// head, at least one resolved name part, and `counter(s)` have been observed.
/// The owning machine remains responsible for number agreement and the rest
/// of the effect, so malformed committed phrases retain precise diagnostics.
pub(in crate::oracle::canonical) fn claims_put_counters_effect(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut probe = cursor.clone();
    if crate::oracle::canonical::automata::consume_lemma(
        &mut probe,
        crate::oracle::canonical::automata::VerbLemma::Put,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )
    .is_err()
        || recognize_counter_count(&mut probe, &EffectRecognitionContext::default()).is_err()
        || recognize_counter_name(&mut probe).is_err()
    {
        return false;
    }
    probe.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
    })
}

fn recognize_counter_count(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<NumericExpression> {
    if context.source_cast_x
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("x") || raw.eq_ignore_ascii_case("half"))
    {
        return recognize_source_cast_x_numeric_expression(cursor);
    }

    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        // `that many`: the quantity bound by the enclosing triggering event.
        return recognize_that_many_event_value(cursor, None);
    }

    let atom = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor
            .peek()
            .ok_or_else(|| cursor.fail("a positive counter quantity"))?
            .to_string();
        cursor.consume(&article)?;
        NumericAtom::Integer(1)
    } else {
        recognize_numeric_atom(cursor)?
    };
    cursor.require(
        !matches!(atom, NumericAtom::Integer(value) if value <= 0),
        "a positive counter quantity",
    )?;
    Ok(NumericExpression::Atom(atom))
}

/// Recognizes one independent `put COUNT COUNTER counters on PERMANENT-OPERAND` effect.
/// Numeric provenance and source anaphora are injected by the ability parent,
/// while this machine owns only counter placement syntax.
pub(in crate::oracle::canonical) fn recognize_put_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    let mut state = PutCountersState::Action;
    let mut memory = PutCountersMemory::default();
    loop {
        state = match state {
            PutCountersState::Action => {
                cursor.enter("effect.put-counters.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                PutCountersState::Count
            }
            PutCountersState::Count => {
                cursor.enter("effect.put-counters.count");
                memory.count = Some(recognize_counter_count(cursor, context)?);
                PutCountersState::CounterKind
            }
            PutCountersState::CounterKind => {
                cursor.enter("effect.put-counters.kind");
                memory.counter = Some(recognize_counter_name(cursor)?);
                PutCountersState::CounterNoun
            }
            PutCountersState::CounterNoun => {
                cursor.enter("effect.put-counters.noun");
                let noun = if matches!(
                    memory.count.as_ref(),
                    Some(NumericExpression::Atom(NumericAtom::Integer(1)))
                ) {
                    "counter"
                } else {
                    "counters"
                };
                cursor.consume(noun)?;
                PutCountersState::Relation
            }
            PutCountersState::Relation => {
                cursor.enter("effect.put-counters.relation");
                cursor.consume("on")?;
                PutCountersState::Permanent
            }
            PutCountersState::Permanent => {
                cursor.enter("effect.put-counters.permanent");
                memory.permanent = Some(super::recognize_contextual_permanent_operand(
                    cursor,
                    name_links,
                    name_failure,
                    context,
                )?);
                PutCountersState::Complete
            }
            PutCountersState::Complete => {
                cursor.enter("effect.put-counters.complete");
                return Ok(PutCountersOnObjectEffectNode {
                    permanent: cursor
                        .require_value(memory.permanent, "a typed permanent operand")?,
                    counter: cursor.require_value(memory.counter, "a counter kind")?,
                    count: cursor.require_value(memory.count, "a counter count")?,
                    direction: CounterChangeDirection::Put,
                });
            }
        };
    }
}

/// `double the number of COUNTER counters on PERMANENT-OPERAND` (CR 701.10e):
/// the doubled object is given as many counters of that kind as it already
/// has. The keyword action is a typed expansion into the ordinary
/// counter-placement action whose count is the recipient's own counter count;
/// the permanent operand is the same child `put ... on` reads.
pub(in crate::oracle::canonical) fn recognize_double_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    cursor.enter("effect.double.action");
    cursor.consume("double")?;
    cursor.enter("effect.double.quantity");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("number")),
        "`the number of` counters; doubling power, toughness, life or mana needs its own typed expansion",
    )?;
    cursor.consume_phrase(&["the", "number", "of"])?;
    cursor.enter("effect.double.counter-kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("effect.double.counter-noun");
    cursor.consume("counters")?;
    cursor.enter("effect.double.relation");
    cursor.consume("on")?;
    cursor.enter("effect.double.permanent");
    let permanent =
        super::recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    Ok(PutCountersOnObjectEffectNode {
        permanent,
        count: NumericExpression::CountersOnRecipient {
            counter: counter.clone(),
        },
        counter,
        direction: CounterChangeDirection::Put,
    })
}

/// True when `move COUNT [KIND] counter(s) from ...` starts here.
pub(in crate::oracle::canonical) fn claims_move_counters_effect(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut probe = cursor.clone();
    if crate::oracle::canonical::automata::consume_lemma(
        &mut probe,
        crate::oracle::canonical::automata::VerbLemma::Move,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )
    .is_err()
    {
        return false;
    }
    (0..4).any(|offset| {
        probe.peek_ahead(offset).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
        })
    })
}

/// `move COUNT KIND counter(s) from PERMANENT-OPERAND onto PERMANENT-OPERAND`
/// (CR 122.5): the removal reads exactly the `remove ... from` children; the
/// destination is the permanent operand `put ... on` reads.
pub(in crate::oracle::canonical) fn recognize_move_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<super::MoveCountersEffectNode> {
    cursor.enter("effect.move-counters.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Move,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    let removal = recognize_counters_from(cursor, name_links, name_failure, context)?;
    cursor.enter("effect.move-counters.destination-relation");
    cursor.consume("onto")?;
    cursor.enter("effect.move-counters.destination");
    let destination =
        super::recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    Ok(super::MoveCountersEffectNode {
        removal,
        destination,
    })
}

/// True when `remove COUNT [KIND] counter(s) from ...` starts here: the
/// `remove` lemma of the verb lexicon in its counters-from frame.
pub(in crate::oracle::canonical) fn claims_remove_counters_effect(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut probe = cursor.clone();
    if crate::oracle::canonical::automata::consume_lemma(
        &mut probe,
        crate::oracle::canonical::automata::VerbLemma::Remove,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )
    .is_err()
    {
        return false;
    }
    (0..4).any(|offset| {
        probe.peek_ahead(offset).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
        })
    })
}

/// `remove COUNT KIND counter(s) from PERMANENT-OPERAND` (CR 122.8): the
/// counter action of `put … on`, read with the same count, counter-name and
/// permanent-operand children, in the removal direction.
pub(in crate::oracle::canonical) fn recognize_remove_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    cursor.enter("effect.remove-counters.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Remove,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    recognize_counters_from(cursor, name_links, name_failure, context)
}

/// `COUNT KIND counter(s) from PERMANENT-OPERAND`: the arguments shared by
/// `remove` and `move`, in the removal direction.
fn recognize_counters_from(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    cursor.enter("effect.remove-counters.count");
    // `all KIND counters` (universal determiner): every counter of that kind
    // the operand has, i.e. the recipient's own count of that kind, the
    // count `double the number of` reads (CR 122.8).
    let universal = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"));
    let count = if universal {
        cursor.consume("all")?;
        None
    } else {
        Some(recognize_counter_count(cursor, context)?)
    };
    cursor.enter("effect.remove-counters.kind");
    if universal
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("counters"))
    {
        // `all counters`: every kind at once has no one counter name.
        return Err(
            cursor.fail("a counter kind after `all` (counters of every kind are not one removal)")
        );
    }
    let counter = recognize_counter_name(cursor)?;
    let count = count.unwrap_or_else(|| NumericExpression::CountersOnRecipient {
        counter: counter.clone(),
    });
    cursor.enter("effect.remove-counters.noun");
    let singular = matches!(count, NumericExpression::Atom(NumericAtom::Integer(1)));
    cursor.consume(if singular { "counter" } else { "counters" })?;
    cursor.enter("effect.remove-counters.relation");
    cursor.consume("from")?;
    cursor.enter("effect.remove-counters.permanent");
    let permanent =
        super::recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    Ok(PutCountersOnObjectEffectNode {
        permanent,
        counter,
        count,
        direction: CounterChangeDirection::Remove,
    })
}

/// True when `distribute COUNT KIND counters among ...` starts here: the
/// `distribute` lemma with its counter noun before `among`.
pub(in crate::oracle::canonical) fn claims_distribute_counters_effect(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut probe = cursor.clone();
    if crate::oracle::canonical::automata::consume_lemma(
        &mut probe,
        crate::oracle::canonical::automata::VerbLemma::Distribute,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )
    .is_err()
    {
        return false;
    }
    let mut counter_seen = false;
    for raw in (0..).map_while(|offset| probe.peek_ahead(offset)) {
        if matches!(raw, "." | "," | ";" | ":") {
            return false;
        }
        if raw.eq_ignore_ascii_case("among") {
            return counter_seen;
        }
        counter_seen |= raw.eq_ignore_ascii_case("counters");
    }
    false
}

/// `distribute COUNT KIND counters among TARGETS` (CR 601.2d): the counters
/// are divided among the declared targets as the spell or ability is put on
/// the stack, each target receiving at least one. The count, counter name
/// and target list are the shared children of `put ... on`; only the
/// division (`CounterChangeDirection::Divided`) is this frame's.
///
/// ```text
/// Targets := Cardinals? TargetList          -- `one, two, or three target creatures`
/// Cardinals := N ("," N)* "," "or"          -- consecutive literals, a range
/// ```
pub(in crate::oracle::canonical) fn recognize_distribute_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    cursor.enter("effect.distribute-counters.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Distribute,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("effect.distribute-counters.count");
    let count = recognize_counter_count(cursor, context)?;
    cursor.require(
        !matches!(count, NumericExpression::Atom(NumericAtom::Integer(1))),
        "several counters to distribute",
    )?;
    cursor.enter("effect.distribute-counters.kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("effect.distribute-counters.noun");
    cursor.consume("counters")?;
    cursor.enter("effect.distribute-counters.among");
    cursor.consume("among")?;
    cursor.enter("effect.distribute-counters.cardinals");
    let enumerated_minimum = recognize_cardinal_enumeration(cursor)?;
    cursor.enter("effect.distribute-counters.targets");
    let mut targets =
        super::recognize_target_list_with_counts(cursor, super::TargetCountSupport::Expressed)?;
    cursor.enter("effect.distribute-counters.target-slot");
    let [slot] = targets.slots.as_mut_slice() else {
        return Err(cursor.fail("one target declaration that receives the divided counters"));
    };
    cursor.require(
        super::selector_guarantees_permanent(&slot.selector),
        "target permanents for the divided counters",
    )?;
    let literal_count = match &count {
        NumericExpression::Atom(NumericAtom::Integer(value)) => u32::try_from(*value).ok(),
        _ => None,
    };
    if let Some(declared) = slot.count.take() {
        // `any number of target creatures`: each chosen target receives at
        // least one counter, so at most COUNT targets can be chosen.
        match (declared.literal(), declared.maximum, literal_count) {
            (Some(cardinality), _, _) => slot.cardinality = cardinality,
            (None, super::TargetCountBound::Unbounded, Some(count)) => {
                slot.cardinality = super::TargetCardinality {
                    minimum: 0,
                    maximum: count,
                };
            }
            _ => {
                return Err(cursor.fail(
                    "a literal number of targets, or a literal number of counters for `any number of` targets",
                ));
            }
        }
    }
    if let Some(minimum) = enumerated_minimum {
        cursor.require(
            minimum < slot.cardinality.minimum
                && slot.cardinality.minimum == slot.cardinality.maximum,
            "an increasing enumeration of target counts (`one, two, or three`)",
        )?;
        slot.cardinality.minimum = minimum;
    }
    if let Some(count) = literal_count {
        cursor.require(
            slot.cardinality.minimum <= count,
            "no more required targets than counters to divide",
        )?;
    }
    Ok(PutCountersOnObjectEffectNode {
        permanent: super::PermanentOperandNode::Targets(targets),
        counter,
        count,
        direction: CounterChangeDirection::Divided,
    })
}

/// `N, N+1, ..., or` before the last count of a target list: consecutive
/// positive literals naming a range of target counts (`one, two, or three
/// target creatures`). Returns the first literal; the target list reads the
/// last one. `None` (nothing consumed) when no enumeration starts here.
fn recognize_cardinal_enumeration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<u32>> {
    let literal_at = |cursor: &OracleTokenCursor<'_>| {
        let mut probe = cursor.clone();
        match recognize_numeric_atom(&mut probe) {
            Ok(NumericAtom::Integer(value)) => u32::try_from(value).ok().filter(|value| *value > 0),
            _ => None,
        }
    };
    if literal_at(cursor).is_none() || cursor.peek_ahead(1) != Some(",") {
        return Ok(None);
    }
    let first = literal_at(cursor).expect("checked literal");
    let mut previous = first;
    recognize_numeric_atom(cursor)?;
    cursor.consume(",")?;
    loop {
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.consume("or")?;
            let last = literal_at(cursor)
                .ok_or_else(|| cursor.fail("the last target count of the enumeration"))?;
            cursor.require(last == previous + 1, "consecutive target counts")?;
            return Ok(Some(first));
        }
        let next =
            literal_at(cursor).ok_or_else(|| cursor.fail("a target count of the enumeration"))?;
        cursor.require(next == previous + 1, "consecutive target counts")?;
        previous = next;
        recognize_numeric_atom(cursor)?;
        cursor.consume(",")?;
    }
}

#[cfg(test)]
#[path = "keyword_action_verb_tests.rs"]
mod keyword_action_verb_tests;
