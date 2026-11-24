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
    kinds: CounterKindsNode,
}

/// One `COUNT KIND counter(s)` placement of a counter action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterPlacementNode {
    pub counter: crate::oracle::counter_name::CounterName,
    pub count: NumericExpression,
}

/// The counter kinds one counter action places besides its `counter` /
/// `count` pair (CR 122.1). The object operand is read once and shared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterKindsNode {
    /// Exactly `count` `counter` counters.
    #[default]
    Single,
    /// `a +1/+1 counter and a flying counter on X`: the first pair, then
    /// each listed pair, all placed on the same object(s).
    Listed(Vec<CounterPlacementNode>),
    /// `your choice of a flying counter or a hexproof counter on X`: the
    /// controller chooses one placement among the first pair and these
    /// while the action resolves (CR 608.2d); only that one is placed.
    ChosenAmong(Vec<CounterPlacementNode>),
}

impl CounterKindsNode {
    pub(in crate::oracle::canonical) fn is_single(&self) -> bool {
        matches!(self, Self::Single)
    }
}

/// True at `your choice of`: the head of a choice among counter kinds.
fn counter_kind_choice_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["your", "choice", "of"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// `counter` agrees with a literal one, `counters` with any other count.
fn counter_noun_for(count: &NumericExpression) -> &'static str {
    if matches!(count, NumericExpression::Atom(NumericAtom::Integer(1))) {
        "counter"
    } else {
        "counters"
    }
}

/// `COUNT KIND counter(s)`: one complete placement with its own noun.
fn recognize_counter_placement(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<CounterPlacementNode> {
    cursor.enter("effect.put-counters.listed.count");
    let count = recognize_counter_count(cursor, context)?;
    cursor.enter("effect.put-counters.listed.kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("effect.put-counters.listed.noun");
    cursor.consume(counter_noun_for(&count))?;
    Ok(CounterPlacementNode { counter, count })
}

/// `, COUNT KIND counter(s)[,] and COUNT KIND counter(s)` after the first
/// placement: a coordinated list of further placements on the same object.
/// The list ends before `on`; every member has its own count and noun.
fn recognize_listed_counter_placements(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<Vec<CounterPlacementNode>> {
    let mut placements = Vec::new();
    loop {
        cursor.enter("effect.put-counters.listed.connector");
        let comma = cursor.peek() == Some(",");
        if comma {
            cursor.consume(",")?;
        }
        let conjunction = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"));
        if conjunction {
            cursor.consume("and")?;
        }
        cursor.require(
            comma || conjunction,
            "`,` or `and` between counter placements",
        )?;
        placements.push(recognize_counter_placement(cursor, context)?);
        if conjunction {
            return Ok(placements);
        }
    }
}

/// `your choice of OPTION (, OPTION)* [,] or OPTION`: each option is
/// `[COUNT] KIND [counter(s)]`. An option without a count keeps the count
/// before it; an option without its noun shares the noun of the last one
/// (`a +1/+1, first strike, vigilance, or menace counter`), so every option
/// before a nounless one is nounless too. The kinds are distinct.
fn recognize_counter_kind_choice(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<Vec<CounterPlacementNode>> {
    cursor.enter("effect.put-counters.choice.head");
    cursor.consume_phrase(&["your", "choice", "of"])?;
    let mut options: Vec<CounterPlacementNode> = Vec::new();
    let mut nouns = Vec::new();
    let mut count: Option<NumericExpression> = None;
    loop {
        cursor.enter("effect.put-counters.choice.option");
        let counted = count.is_none()
            || cursor.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("a")
                    || raw.eq_ignore_ascii_case("an")
                    || raw.eq_ignore_ascii_case("that")
            })
            || recognize_numeric_atom(&mut cursor.clone()).is_ok();
        if counted {
            count = Some(recognize_counter_count(cursor, context)?);
        }
        let option_count = cursor.require_value(count.clone(), "a counter quantity")?;
        cursor.enter("effect.put-counters.choice.kind");
        let counter = recognize_counter_name(cursor)?;
        cursor.require(
            !options.iter().any(|option| option.counter == counter),
            "distinct counter kinds to choose among",
        )?;
        let has_noun = cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
        });
        if has_noun {
            cursor.enter("effect.put-counters.choice.noun");
            cursor.consume(counter_noun_for(&option_count))?;
        }
        nouns.push(has_noun);
        options.push(CounterPlacementNode {
            counter,
            count: option_count,
        });
        cursor.enter("effect.put-counters.choice.connector");
        let comma = cursor.peek() == Some(",");
        if comma {
            cursor.consume(",")?;
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.consume("or")?;
            // The last option, then the list is complete.
            cursor.enter("effect.put-counters.choice.last");
            let counted = cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                || recognize_numeric_atom(&mut cursor.clone()).is_ok();
            if counted {
                count = Some(recognize_counter_count(cursor, context)?);
            }
            let option_count = cursor.require_value(count.clone(), "a counter quantity")?;
            let counter = recognize_counter_name(cursor)?;
            cursor.require(
                !options.iter().any(|option| option.counter == counter),
                "distinct counter kinds to choose among",
            )?;
            cursor.enter("effect.put-counters.choice.noun");
            cursor.consume(counter_noun_for(&option_count))?;
            options.push(CounterPlacementNode {
                counter,
                count: option_count,
            });
            // A shared noun closes the whole list: the options before the
            // last one all lack their own noun, or all have it.
            cursor.require(
                nouns.iter().all(|noun| *noun) || nouns.iter().all(|noun| !*noun),
                "one noun per option or one noun shared by all options",
            )?;
            cursor.require(options.len() >= 2, "at least two counter kinds")?;
            return Ok(options);
        }
        cursor.require(comma, "`,` or `or` between counter kinds")?;
    }
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
    {
        return false;
    }
    // `put your choice of COUNT KIND ...`: a choice among counter kinds.
    if counter_kind_choice_starts(&probe) {
        return true;
    }
    if recognize_counter_count(&mut probe, &EffectRecognitionContext::default()).is_err()
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

/// Recognizes one independent `put COUNT COUNTER counters on PERMANENT-OPERAND` effect
/// with exactly one counter kind: the contract of parents whose own lowering
/// reads one `counter`/`count` pair. A listed or chosen kind fails here,
/// explicitly, instead of being dropped by such a parent.
pub(in crate::oracle::canonical) fn recognize_put_counters_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    let node = recognize_put_counter_placements_effect(cursor, name_links, name_failure, context)?;
    cursor.enter("effect.put-counters.kinds-support");
    cursor.require(
        node.kinds.is_single(),
        "one counter kind for this parent (listed or chosen kinds are read by the effect sequence)",
    )?;
    Ok(node)
}

/// Recognizes one `put PLACEMENTS on PERMANENT-OPERAND` effect: one
/// placement, a coordinated list of placements (`a +1/+1 counter and a
/// flying counter`) or a choice among kinds (`your choice of a flying
/// counter or a hexproof counter`). Numeric provenance and source anaphora
/// are injected by the ability parent, while this machine owns only counter
/// placement syntax. The effect sequence lowers every `CounterKindsNode`.
pub(in crate::oracle::canonical) fn recognize_put_counter_placements_effect(
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
                if counter_kind_choice_starts(cursor) {
                    // `your choice of`: the options are complete placements;
                    // the first is the node's own pair, the rest the choice.
                    let mut options = recognize_counter_kind_choice(cursor, context)?.into_iter();
                    let first = options
                        .next()
                        .ok_or_else(|| cursor.fail("a first counter option"))?;
                    memory.counter = Some(first.counter);
                    memory.count = Some(first.count);
                    memory.kinds = CounterKindsNode::ChosenAmong(options.collect());
                    PutCountersState::Relation
                } else {
                    memory.count = Some(recognize_counter_count(cursor, context)?);
                    PutCountersState::CounterKind
                }
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
                if memory.kinds.is_single()
                    && (cursor.peek() == Some(",")
                        || cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("and")))
                {
                    // `a +1/+1 counter and a flying counter on X`: further
                    // placements share the one object operand below.
                    memory.kinds = CounterKindsNode::Listed(recognize_listed_counter_placements(
                        cursor, context,
                    )?);
                }
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
                    kinds: memory.kinds,
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
        kinds: CounterKindsNode::Single,
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
        kinds: CounterKindsNode::Single,
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
        kinds: CounterKindsNode::Single,
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
