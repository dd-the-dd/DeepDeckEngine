//! The shared object operand of the verb argument frames `ObjectSelection`
//! and `ObjectToZone` (destroy, exile, return, put, tap, untap ...).
//!
//! ```text
//! Operand := Reference                       (`it`, `that creature`, `the creature`)
//!          | CardTargets                     (`target creature card ... from ZONE`)
//!          | LibraryPosition                 (`the top N cards of PLAYER's library`)
//!          | TargetList                      (`target`, `N target`, `X target`,
//!                                             `any number of target`, `and` slots)
//!          | QuantifiedQuery                 (`each|all|a|an|another|N|up to N`
//!                                             QUERY: a collection or a choice)
//! ```
//!
//! One reader, tried in that order; each alternative is a transactional
//! child that consumes nothing unless it claims the operand. The reference
//! is resolved by the shared binding environment (never by a local pronoun
//! table), the query by the shared object noun phrase, the target list by
//! the shared target-list automaton and its count node, the card target and
//! library position by their card-collection children. The verb frame only
//! type-checks the typed result against what its action can lower.
use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::*;

/// The typed operand one verb frame received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum VerbObjectOperandNode {
    /// An object the binding environment names (CR 608.2c): a declared
    /// target, the source, an event object or a moved card.
    Reference(PermanentEffectReference),
    /// Declared permanent (or spell) targets (CR 601.2c, 115.1).
    Targets(TargetListNode),
    /// Declared card targets in a zone an explicit `from` origin names.
    CardTargets(CardTargetOperandNode),
    /// Chosen while the effect resolves, not targeted (CR 608.2d).
    Selection(PermanentSelectionOperandNode),
    /// Every member of a battlefield query, no choice.
    Collection {
        objects: ObjectQueryNode,
        exclude_source: bool,
    },
    /// A position in a library (CR 401.2).
    LibraryTop(CardLibraryTopNode),
}

impl VerbObjectOperandNode {
    /// Whether the operand may denote more than one object; possessive
    /// agreement (`its` / `their owner's`) follows it.
    pub(in crate::oracle::canonical) fn is_plural(&self) -> bool {
        match self {
            Self::Reference(_) => false,
            Self::Targets(targets) => {
                targets.slots.len() > 1
                    || targets.slots.iter().any(|slot| {
                        slot.count
                            .as_ref()
                            .map_or(slot.cardinality.maximum > 1, |count| count.is_plural())
                    })
            }
            Self::CardTargets(target) => {
                target.unbounded || target.cardinality.maximum != NumericAtom::Integer(1)
            }
            Self::Selection(selection) => selection.is_plural(),
            Self::Collection { .. } | Self::LibraryTop(_) => true,
        }
    }
}

/// What a verb frame admits besides declared targets. Every frame admits
/// references, targets and the quantified query; the zone alternatives are
/// only read by an object-to-zone frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct VerbObjectOperandExpectation {
    pub counts: TargetCountSupport,
    /// `target ... card from ZONE` and `the top N cards of ...`.
    pub card_zones: bool,
}

impl VerbObjectOperandExpectation {
    pub(in crate::oracle::canonical) const OBJECT_TO_ZONE: Self = Self {
        counts: TargetCountSupport::Expressed,
        card_zones: true,
    };
}

/// Reads the operand of one object-taking verb, the verb already consumed.
pub(in crate::oracle::canonical) fn recognize_verb_object_operand(
    cursor: &mut OracleTokenCursor<'_>,
    expectation: VerbObjectOperandExpectation,
) -> MachineResult<VerbObjectOperandNode> {
    cursor.enter("verb-operand.head");
    // (1) A determined or anaphoric reference, through the shared binding
    // environment the enclosing program installed.
    if definite_object_anaphor_starts(cursor) {
        cursor.enter("verb-operand.reference.definite");
        let environment = installed_binding_environment(cursor);
        return recognize_definite_object_reference(cursor, &environment)
            .map(VerbObjectOperandNode::Reference);
    }
    if bound_object_reference_starts(cursor) {
        cursor.enter("verb-operand.reference");
        let environment = installed_binding_environment(cursor);
        return recognize_bound_object_reference(cursor, &environment)
            .map(VerbObjectOperandNode::Reference);
    }
    if expectation.card_zones {
        // (2a) Card targets: committed once their `from` origin is read.
        cursor.enter("verb-operand.card-targets");
        match try_recognize_card_target_operand(cursor) {
            CollectionOperandAttempt::Recognized(fragment) => {
                return Ok(VerbObjectOperandNode::CardTargets(fragment.node));
            }
            CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
            CollectionOperandAttempt::NotClaimed
            | CollectionOperandAttempt::RejectedCandidate(_) => {}
        }
        // (2b) A library position: committed after `the top`.
        cursor.enter("verb-operand.library-position");
        match try_recognize_card_library_position(cursor, true) {
            CollectionOperandAttempt::Recognized(fragment) => {
                return Ok(VerbObjectOperandNode::LibraryTop(fragment.node));
            }
            CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
            CollectionOperandAttempt::NotClaimed
            | CollectionOperandAttempt::RejectedCandidate(_) => {}
        }
    }
    // (3) Declared targets. A possessive `target player's` owns a zone and
    // is never an object target slot.
    // A count that heads no `target` (`any number of Mountains you
    // control`, `one or more lands`) is a resolution choice (CR 608.2d),
    // read below by the quantified query; the same count before `target`
    // is a declaration.
    if !starts_possessive_target_player(cursor)
        && (target_operand_starts(cursor)
            || (expectation.counts == TargetCountSupport::Expressed
                && expressed_target_count_starts(cursor)
                && !resolution_selection_count_starts(cursor)))
    {
        cursor.enter("verb-operand.targets");
        return recognize_target_list_with_counts(cursor, expectation.counts)
            .map(VerbObjectOperandNode::Targets);
    }
    // An attached-object reference (`enchanted creature`) names one object
    // through the source's attachment; it is never a collection query.
    if attached_object_reference_starts(cursor) {
        cursor.enter("verb-operand.attached-reference");
        return Err(cursor.fail(
            "an attached-object reference read by the permanent subject frame, not a query",
        ));
    }
    // (4) A quantified non-target query: a universal collection or a
    // resolution choice.
    cursor.enter("verb-operand.query");
    let operand = recognize_quantified_permanent_operand(cursor)?;
    // The operand ends transactionally: a relative clause the shared query
    // did not read is this operand's failure, not the end of the operand.
    cursor.enter("verb-operand.query.tail");
    refuse_unread_relative_tail(cursor)?;
    Ok(operand)
}

/// `one or more QUERY` (CR 608.2d): at least one, no upper bound. The noun
/// phrase reads the query as a bare plural; the count is this reader's.
fn recognize_at_least_one_selection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<VerbObjectOperandNode> {
    cursor.enter("verb-operand.query.at-least-one");
    cursor.consume_phrase(&["one", "or", "more"])?;
    let phrase = recognize_object_noun_phrase(
        cursor,
        ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Permanent]),
    )?;
    cursor.enter("verb-operand.query.at-least-one.domain");
    cursor.require(
        phrase.determiner.is_none() && phrase.domain == Some(ObjectKind::Permanent),
        "a plural battlefield-permanent query after `one or more`",
    )?;
    let exclude_source = phrase.exclude_source;
    let mut objects = phrase.query.into_object_query(cursor)?;
    objects.exclude_source = exclude_source;
    Ok(VerbObjectOperandNode::Selection(
        PermanentSelectionOperandNode {
            bounds: Some(TargetCount {
                minimum: NumericExpression::from(1),
                maximum: TargetCountBound::Unbounded,
            }),
            query: objects,
        },
    ))
}

/// `target`, `another target`, a literal count before `[other] target`, or
/// `up to N [other] target`. A bare `another` / `up to N` without `target`
/// is a resolution choice and belongs to the quantified query.
fn target_operand_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut probe = cursor.clone();
    if probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
    {
        return true;
    }
    if probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("any"))
    {
        return probe
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"));
    }
    if probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("another"))
    {
        return probe
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"));
    }
    if !starts_explicit_target_slot(&probe) {
        return false;
    }
    // A count: the target head must follow it (`two target`, `up to two
    // other target`, `one or two target`).
    if probe.attempt(recognize_target_cardinality).is_err() {
        return false;
    }
    if probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
    {
        let _ = probe.consume("other");
    }
    probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
}

/// `each|all [other] QUERY`, a bare plural, or `a|an|another|N|up to N
/// QUERY` over battlefield permanents. The shared object noun phrase reads
/// the determiner, the exclusion and the query; this parent type-checks.
fn recognize_quantified_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<VerbObjectOperandNode> {
    if ["one", "or", "more"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
    {
        return recognize_at_least_one_selection(cursor);
    }
    let phrase = recognize_object_noun_phrase(
        cursor,
        ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Permanent]),
    )?;
    cursor.enter("verb-operand.query.domain");
    cursor.require(
        phrase.domain == Some(ObjectKind::Permanent),
        "an explicitly battlefield-permanent query, not players, spells or cards",
    )?;
    let exclude_source = phrase.exclude_source;
    let determiner = phrase.determiner.clone();
    let mut objects = phrase.query.into_object_query(cursor)?;
    objects.exclude_source = exclude_source;
    match determiner {
        // A bare plural is not an operand quantifier of an action verb:
        // its universal reading is not proved for these frames.
        None => {
            cursor.enter("verb-operand.query.determiner");
            Err(cursor.fail(
                "a quantifier (`each`, `all`, `a`, `an`, `another`, a count or `up to` a count)",
            ))
        }
        Some(ObjectDeterminer::Universal { .. }) => {
            cursor.enter("verb-operand.query.collection");
            Ok(VerbObjectOperandNode::Collection {
                objects,
                exclude_source,
            })
        }
        Some(ObjectDeterminer::Chosen { quantity, .. }) => {
            cursor.enter("verb-operand.query.selection");
            // The one count node shared with declarations; the amount is the
            // shared numeric expression, validated in the face context when
            // the choice is lowered (never defaulted).
            let bounds = match quantity {
                ChosenObjectQuantity::Exactly(amount) => TargetCount {
                    minimum: amount.clone(),
                    maximum: TargetCountBound::Amount(amount),
                },
                ChosenObjectQuantity::UpTo(amount) => TargetCount {
                    minimum: NumericExpression::from(0),
                    maximum: TargetCountBound::Amount(amount),
                },
                // `any number of QUERY`: zero or more, no upper bound.
                ChosenObjectQuantity::AnyNumber => TargetCount {
                    minimum: NumericExpression::from(0),
                    maximum: TargetCountBound::Unbounded,
                },
            };
            cursor.require(
                !matches!(
                    &bounds.maximum,
                    TargetCountBound::Amount(NumericExpression::Atom(NumericAtom::Integer(value)))
                        if *value <= 0
                ),
                "a positive resolution quantity",
            )?;
            Ok(VerbObjectOperandNode::Selection(
                PermanentSelectionOperandNode {
                    bounds: Some(bounds),
                    query: objects,
                },
            ))
        }
        Some(ObjectDeterminer::Target { .. }) => Err(cursor
            .fail("a target head read by the shared target-list child, not by the object query")),
    }
}

#[cfg(test)]
#[path = "verb_object_operand_tests.rs"]
mod tests;
