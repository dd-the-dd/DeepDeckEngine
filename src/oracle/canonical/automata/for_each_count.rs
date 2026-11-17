//! Shared `for each COUNTABLE` numeric child (CR 107.3 style multiplier).
//!
//! The child owns only the counted operand. The parent that owns the action
//! (an effect sequence, a mana production or a continuous modifier) decides
//! which quantity the count multiplies, so no action-specific recognizer is
//! needed for a new combination.
//!
//! ```text
//! ForEach  := "for" "each" Countable
//! Countable := ["other"] permanent collection      (countPermanents)
//!            | counter-name "counter(s)" "on this" NOUN   (countCounters)
//!            | "color" "among" ["other"] collection (countDistinctColors)
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// True when the cursor is at the `for each` multiplier introducer.
pub(in crate::oracle::canonical) fn for_each_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("for"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
}

fn counter_operand_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .token_slice(cursor.position(), cursor.position() + 1)
        .first()
        .is_some_and(|token| {
            matches!(
                token.token_type.as_str(),
                "counterNamePart" | "counterValue"
            )
        })
}

fn predicate_needs_declaration(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => {
            right.requires_target_binding() || right.uses_x()
        }
        ObjectPredicate::Not(child) => predicate_needs_declaration(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            children.iter().any(predicate_needs_declaration)
        }
        _ => false,
    }
}

/// Reads one `for each COUNTABLE` multiplier and returns the count as a typed
/// numeric expression. Unsupported countables fail with a precise state.
pub(in crate::oracle::canonical) fn recognize_for_each_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.quantifier");
    cursor.consume_phrase(&["for", "each"])?;
    recognize_counted_collection(cursor, "color")
}

/// True when the cursor is at the `the number of` quantity introducer.
pub(in crate::oracle::canonical) fn number_of_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["the", "number", "of"].iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    })
}

/// Reads `the number of COUNTABLE`. The countable is the same typed child as
/// the `for each` multiplier, so both introducers share one count semantics.
pub(in crate::oracle::canonical) fn recognize_number_of_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.number-of.quantifier");
    cursor.consume_phrase(&["the", "number", "of"])?;
    recognize_counted_collection(cursor, "colors")
}

/// The countable operand shared by every counting introducer. `color_noun`
/// is the introducer's grammatical number of the distinct-colors noun.
fn recognize_counted_collection(
    cursor: &mut OracleTokenCursor<'_>,
    color_noun: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.countable");
    if counter_operand_starts(cursor) {
        return recognize_source_counter_operand(cursor);
    }
    let colors = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(color_noun))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("among"));
    if colors {
        cursor.enter("numeric.for-each.colors");
        cursor.consume_phrase(&[color_noun, "among"])?;
    }
    let distinct_names = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("differently"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("named"));
    if distinct_names {
        cursor.enter("numeric.for-each.distinct-names");
        cursor.require(!colors, "one counting measure, not colors and names")?;
        cursor.consume_phrase(&["differently", "named"])?;
    }
    let exclude_source = recognize_optional_source_exclusion(cursor)?;
    cursor.enter("numeric.for-each.collection");
    let query = recognize_object_collection(cursor)?;
    cursor.require(
        query.predicate.has_domain(ObjectKind::Permanent)
            && query.predicate.supports_domain(ObjectKind::Permanent),
        "a counted collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "a counted collection predicate independent of targets and declared variables",
    )?;
    cursor.enter("numeric.for-each.controller");
    let measure = if colors {
        // The engine evaluates distinct colors for one named player only.
        cursor.require(
            query.controller == Some(ObjectControllerScope::AbilityController),
            "a color-counted collection controlled by you",
        )?;
        PermanentCountMeasure::DistinctColors
    } else if distinct_names {
        // The engine evaluates distinct names for one named player only.
        cursor.require(
            query.controller == Some(ObjectControllerScope::AbilityController),
            "a name-counted collection controlled by you",
        )?;
        PermanentCountMeasure::DistinctNames
    } else {
        cursor.require(
            matches!(
                query.controller,
                None | Some(ObjectControllerScope::AbilityController)
            ),
            "a counted collection controlled by you or by any player",
        )?;
        PermanentCountMeasure::Objects
    };
    Ok(NumericExpression::PermanentCount(Box::new(
        PermanentCountNode {
            predicate: query.predicate,
            controller: query.controller,
            exclude_source,
            measure,
        },
    )))
}

/// Multiplies an action's literal base quantity by a shared count. A base of
/// one is the count itself; any other positive literal is an explicit product.
/// A non-literal base (a variable, an expression) has no proved reading of
/// `N for each`, so it is refused instead of reinterpreted.
pub(in crate::oracle::canonical) fn scale_by_for_each_count(
    base: &NumericExpression,
    count: NumericExpression,
) -> Option<NumericExpression> {
    match base {
        NumericExpression::Atom(NumericAtom::Integer(1)) => Some(count),
        NumericExpression::Atom(NumericAtom::Integer(amount)) if *amount > 1 => Some(
            NumericExpression::multiply(NumericExpression::from(*amount), count),
        ),
        _ => None,
    }
}

/// Applies a trailing `for each` count to the quantity of the action it
/// follows. Only actions whose quantity is the shared numeric expression
/// accept it; every other action is refused so no text is dropped.
pub(in crate::oracle::canonical) fn apply_for_each_multiplier(
    effect: &mut LocalEffectNode,
    count: NumericExpression,
) -> bool {
    let quantity = match effect {
        LocalEffectNode::DrawCards(node) => &mut node.count,
        LocalEffectNode::GainLife(node) => &mut node.amount,
        LocalEffectNode::LoseLife(node) => &mut node.amount,
        LocalEffectNode::PutCounters(node) => &mut node.count,
        LocalEffectNode::PlayerAction(node) => match &mut node.action {
            SingularPlayerAction::Draw(quantity) | SingularPlayerAction::Mill(quantity) => quantity,
            SingularPlayerAction::Life(life) => &mut life.amount,
            SingularPlayerAction::Discard(_) | SingularPlayerAction::Sacrifice(_) => return false,
        },
        _ => return false,
    };
    match scale_by_for_each_count(quantity, count) {
        Some(scaled) => {
            *quantity = scaled;
            true
        }
        None => false,
    }
}
