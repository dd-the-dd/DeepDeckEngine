use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, NumericAtom, NumericExpression, PutCountersOnObjectEffectNode,
    recognize_counter_name, recognize_numeric_atom, recognize_source_cast_x_numeric_expression,
    recognize_source_reference_with_antecedent,
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
    permanent: Option<super::LocalObjectReference>,
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
    if probe.consume("put").is_err()
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

/// Recognizes one independent `put COUNT COUNTER counters on SOURCE` effect.
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
                cursor.consume("put")?;
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
                memory.permanent = Some(recognize_source_reference_with_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    context.source.as_ref(),
                )?);
                PutCountersState::Complete
            }
            PutCountersState::Complete => {
                cursor.enter("effect.put-counters.complete");
                return Ok(PutCountersOnObjectEffectNode {
                    permanent: cursor.require_value(memory.permanent, "a source permanent")?,
                    counter: cursor.require_value(memory.counter, "a counter kind")?,
                    count: cursor.require_value(memory.count, "a counter count")?,
                });
            }
        };
    }
}
