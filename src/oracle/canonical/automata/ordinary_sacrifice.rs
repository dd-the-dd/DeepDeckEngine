//! Ordinary resolution action. Costs retain their separate payer contract.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeEffectNode {
    Source(SacrificeSourceEffectNode),
    Selected {
        object: ObjectQueryNode,
        count: NumericExpression,
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
    if source_or_anaphor_reference_starts_at(probe.position(), probe.peek(), names, name_failure) {
        return recognize_sacrifice_source_effect(cursor, names, name_failure, antecedent)
            .map(SacrificeEffectNode::Source);
    }
    let read = |cursor: &mut OracleTokenCursor<'_>| {
        source_sacrifice::atomic_action::recognize_sacrifice_action(cursor, |cursor| {
            recognize_selected_sacrifice_operand(cursor, SacrificeOperandOwner::AbilityController)
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
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("effect.sacrifice.object-query");
    let object = recognize_object_query_with_shared_cardinality(cursor, &count)?;
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
    Ok(SacrificeEffectNode::Selected { object, count })
}

#[cfg(test)]
#[path = "ordinary_sacrifice_tests.rs"]
mod tests;
