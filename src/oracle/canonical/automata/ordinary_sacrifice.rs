//! Ordinary resolution action. Costs retain their separate payer contract.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeEffectNode {
    Source(SacrificeSourceEffectNode),
    /// `sacrifice N QUERY`, or a range the sacrificing player picks from
    /// (`sacrifice any number of artifacts`, `up to three creatures`).
    Selected {
        object: ObjectQueryNode,
        count: SelectionCount,
    },
}

pub(in crate::oracle::canonical) fn recognize_ordinary_sacrifice_effect(
    cursor: &mut OracleTokenCursor<'_>,
    names: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<SacrificeEffectNode> {
    // Probe only the operand's typed reference head; the original source
    // reader retains aliases, diagnostics, and its exact semantic node.
    let mut probe = cursor.clone();
    crate::oracle::canonical::automata::consume_lemma(
        &mut probe,
        crate::oracle::canonical::automata::VerbLemma::Sacrifice,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    if source_or_anaphor_reference_starts_at(probe.position(), probe.peek(), names, name_failure)
        || (antecedent.is_some() && definite_object_anaphor_starts(&probe))
    {
        return recognize_sacrifice_source_effect(cursor, names, name_failure, antecedent)
            .map(SacrificeEffectNode::Source);
    }
    let read = |cursor: &mut OracleTokenCursor<'_>| {
        source_sacrifice::atomic_action::recognize_sacrifice_action(cursor, |cursor| {
            let operand = recognize_selected_sacrifice_operand(
                cursor,
                SacrificeOperandOwner::AbilityController,
            )?;
            // `of their choice`, kept by a performer's person shift: the
            // acting player chooses, as it always does here (CR 101.4).
            let marker = ["of", "their", "choice"]
                .iter()
                .enumerate()
                .all(|(offset, word)| {
                    cursor
                        .peek_ahead(offset)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
                });
            if marker {
                source_sacrifice::atomic_action::selection_method::recognize_acting_player_choice_marker(
                    cursor,
                )?;
            }
            Ok(operand)
        })
    };
    let action = match cursor.attempt(read) {
        Ok(action) => action,
        // `sacrifice a creature, an artifact, and a land`: each headed object
        // is its own quantity. The first operand ends at the next headed
        // object; the sequence reads the others with the verb elided.
        Err(failure) => match independent_sacrifice_operand_end(cursor) {
            Some(end) => cursor.bounded(end, read)?,
            None => return Err(failure),
        },
    };
    Ok(action.operand)
}

fn is_indefinite_article(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
}

/// The position of `, a|an`, `, and a|an` or `and a|an` that opens another
/// independently headed object in the current sentence, if any.
fn independent_sacrifice_operand_end(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    let raw = |index: usize| tokens.get(index).map(|token| token.raw.as_str());
    (0..tokens.len())
        .take_while(|index| !matches!(raw(*index), Some("." | ":" | ";" | "(")))
        .find(|index| match raw(*index) {
            Some(",") => {
                raw(index + 1).is_some_and(is_indefinite_article)
                    || (raw(index + 1).is_some_and(|word| word.eq_ignore_ascii_case("and"))
                        && raw(index + 2).is_some_and(is_indefinite_article))
            }
            Some(word) if word.eq_ignore_ascii_case("and") => {
                raw(index + 1).is_some_and(is_indefinite_article)
            }
            _ => false,
        })
        .filter(|index| *index > 0)
        .map(|index| cursor.position() + index)
}

/// One elided-verb operand of a sacrifice list, bounded like the first.
pub(in crate::oracle::canonical) fn recognize_elided_sacrifice_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SacrificeEffectNode> {
    cursor.enter("effect.sacrifice.elided-operand");
    let read = |cursor: &mut OracleTokenCursor<'_>| {
        recognize_selected_sacrifice_operand(cursor, SacrificeOperandOwner::AbilityController)
    };
    match cursor.attempt(read) {
        Ok(operand) => Ok(operand),
        Err(failure) => match independent_sacrifice_operand_end(cursor) {
            Some(end) => cursor.bounded(end, read),
            None => Err(failure),
        },
    }
}

/// True after `sacrifice OBJECT` and a list connector when another headed
/// object follows: the verb is elided (`..., an artifact, and a land`).
pub(in crate::oracle::canonical) fn elided_sacrifice_operand_starts(
    cursor: &OracleTokenCursor<'_>,
    previous: Option<&LocalEffectNode>,
) -> bool {
    matches!(
        previous,
        Some(LocalEffectNode::Sacrifice(
            SacrificeEffectNode::Selected { .. }
        ))
    ) && cursor.peek().is_some_and(is_indefinite_article)
}

/// Whose permanents the operand ranges over. The acting player of an ordinary
/// effect is the ability controller, who may restate it (`a creature you
/// control`). A quantified player acts on its own permanents, so any explicit
/// controller relation would contradict the iteration binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeOperandOwner {
    AbilityController,
    ActingPlayer,
}

/// Shared quantity plus object-query operand of a selected sacrifice.
pub(in crate::oracle::canonical) fn recognize_selected_sacrifice_operand(
    cursor: &mut OracleTokenCursor<'_>,
    owner: SacrificeOperandOwner,
) -> MachineResult<SacrificeEffectNode> {
    cursor.enter("effect.sacrifice.quantity");
    // A range the sacrificing player picks from (`any number of`, `one or
    // more`, `up to N`) is the shared quantity slot. Only the ability's
    // controller chooses one here: a quantified player's own sacrifice keeps
    // its stated count.
    let range = if owner == SacrificeOperandOwner::AbilityController {
        recognize_chosen_quantity_range(cursor)?
    } else {
        None
    };
    // The shared object determiner reads a stated count; the parent only
    // admits a chosen exact count. `another` has no lowered source exclusion
    // here, so it is refused at the determiner instead of failing inside a
    // number.
    // `half the QUERY [of their choice], rounded up|down` (CR 107.1a): the
    // shared partitive quantity over this operand (`quantity_slot.rs`).
    let partitive = range.is_none() && partitive_half_starts(cursor);
    let count = match range {
        Some(range) => SelectionCount::Range(range),
        // A plural quota until the operand the count halves is read.
        None if partitive => {
            recognize_partitive_half(cursor)?;
            SelectionCount::Exactly(NumericExpression::from(2))
        }
        None => match recognize_object_determiner(cursor)? {
            Some(ObjectDeterminer::Chosen {
                quantity: ChosenObjectQuantity::Exactly(count),
                another: false,
            }) => SelectionCount::Exactly(count),
            Some(ObjectDeterminer::Chosen { another: true, .. }) => {
                cursor.enter("effect.sacrifice.source-exclusion");
                return Err(cursor.fail("a sacrifice whose source exclusion this parent lowers"));
            }
            _ => {
                cursor.enter("effect.sacrifice.quantity.determiner");
                return Err(cursor.fail(
                "a sacrifice count: `a`, `an`, a number, X, `any number of`, `one or more` or `up to N`",
            ));
            }
        },
    };
    cursor.enter("effect.sacrifice.object-query");
    let context = ObjectQueryContext::shared_quota(&count.quota());
    // `that player sacrifices half the creatures they control`: the
    // restated `they` is the acting player whose permanents are halved.
    let mut query = if partitive && owner == SacrificeOperandOwner::ActingPlayer {
        super::read_with_acting_player(cursor, |cursor| {
            recognize_object_noun_query(cursor, context)
        })?
    } else {
        recognize_object_noun_query(cursor, context)?
    };
    // `a creature other than this creature`: the source is not a candidate.
    let exclude_source = query.take_source_exclusion();
    cursor.require(
        !exclude_source || owner == SacrificeOperandOwner::AbilityController,
        "a source exclusion among the permanents of the source's controller",
    )?;
    let mut object = query.into_object_query(cursor)?;
    object.exclude_source = exclude_source;
    // The acting player restated (`half the creatures they control`) is the
    // acting player's own permanents, the ones its count halves.
    let acting_player = Some(ObjectControllerScope::Player(RelatedPlayer::Bound(
        BoundPlayerReference::ActingPlayer,
    )));
    if partitive
        && owner == SacrificeOperandOwner::ActingPlayer
        && object.controller == acting_player
    {
        object.controller = None;
    }
    cursor.require(
        match owner {
            SacrificeOperandOwner::AbilityController => matches!(
                object.controller,
                None | Some(ObjectControllerScope::AbilityController)
            ),
            SacrificeOperandOwner::ActingPlayer => object.controller.is_none(),
        },
        match owner {
            SacrificeOperandOwner::AbilityController => {
                "a permanent controlled by the resolving ability's controller"
            }
            SacrificeOperandOwner::ActingPlayer => {
                "a permanent query without a controller relation (the acting player's own permanents)"
            }
        },
    )?;
    cursor.require(
        object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "a permanent query with a nominal head",
    )?;
    if partitive {
        cursor.enter("effect.sacrifice.partitive");
        // `of their choice`: the acting player chooses, as it always does.
        if ["of", "their", "choice"]
            .iter()
            .enumerate()
            .all(|(offset, word)| {
                cursor
                    .peek_ahead(offset)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            })
        {
            cursor.consume_phrase(&["of", "their", "choice"])?;
        }
        let rounding = recognize_partitive_rounding(cursor)?;
        let counted_controller = match owner {
            SacrificeOperandOwner::AbilityController => {
                Some(ObjectControllerScope::AbilityController)
            }
            SacrificeOperandOwner::ActingPlayer => acting_player,
        };
        let half = cursor.require_value(
            partitive_half_count(&object, counted_controller, rounding),
            "half the permanents of the sacrificing player",
        )?;
        return Ok(SacrificeEffectNode::Selected {
            object,
            count: SelectionCount::Exactly(half),
        });
    }
    Ok(SacrificeEffectNode::Selected { object, count })
}

#[cfg(test)]
#[path = "ordinary_sacrifice_tests.rs"]
mod tests;
