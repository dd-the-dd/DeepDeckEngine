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
            Self::Selection(selection) => selection
                .bounds
                .is_none_or(|bounds| bounds.maximum != NumericAtom::Integer(1)),
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
    if !starts_possessive_target_player(cursor)
        && (target_operand_starts(cursor)
            || (expectation.counts == TargetCountSupport::Expressed
                && expressed_target_count_starts(cursor)))
    {
        cursor.enter("verb-operand.targets");
        return recognize_target_list_with_counts(cursor, expectation.counts)
            .map(VerbObjectOperandNode::Targets);
    }
    // (4) A quantified non-target query: a universal collection or a
    // resolution choice.
    cursor.enter("verb-operand.query");
    recognize_quantified_permanent_operand(cursor)
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
        None | Some(ObjectDeterminer::Universal { .. }) => {
            cursor.enter("verb-operand.query.collection");
            Ok(VerbObjectOperandNode::Collection {
                objects,
                exclude_source,
            })
        }
        Some(ObjectDeterminer::Chosen { quantity, .. }) => {
            cursor.enter("verb-operand.query.selection");
            let bounds = match quantity {
                ChosenObjectQuantity::Exactly(NumericExpression::Atom(amount)) => {
                    ExpressedTargetCardinality {
                        minimum: amount,
                        maximum: amount,
                    }
                }
                ChosenObjectQuantity::UpTo(NumericExpression::Atom(amount)) => {
                    ExpressedTargetCardinality {
                        minimum: NumericAtom::Integer(0),
                        maximum: amount,
                    }
                }
                ChosenObjectQuantity::Exactly(_) | ChosenObjectQuantity::UpTo(_) => {
                    return Err(cursor.fail(
                        "a literal or X resolution quantity for a chosen permanent operand",
                    ));
                }
                ChosenObjectQuantity::AnyNumber => {
                    return Err(cursor.fail(
                        "a bounded resolution quantity; `any number of` non-target permanents has no selection writer",
                    ));
                }
            };
            cursor.require(
                !matches!(bounds.maximum, NumericAtom::Integer(value) if value <= 0),
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
