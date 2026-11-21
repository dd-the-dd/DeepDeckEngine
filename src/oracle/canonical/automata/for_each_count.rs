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
//!            | collection "in" zone-holders zone        (countCards)
//!            | collection "on the battlefield"          (countPermanents, any controller)
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
    if !colors && !distinct_names && counted_zone_relation_follows(cursor) {
        // CR 400.1: a collection with an explicit `in ZONE` relation counts
        // cards in that zone. The shared scoped query owns the predicate and
        // the zone holders; this child only names the count.
        cursor.enter("numeric.for-each.zone-collection");
        let query = recognize_scoped_object_query(cursor)?;
        cursor.require(
            query.domain == ObjectKind::Card,
            "a card collection in a graveyard, hand or exile zone",
        )?;
        return Ok(NumericExpression::CountQuery(Box::new(query)));
    }
    let exclude_source = recognize_optional_source_exclusion(cursor)?;
    cursor.enter("numeric.for-each.collection");
    let query = recognize_object_collection(cursor)?;
    // CR 110.1: `on the battlefield` names every permanent there, whoever
    // controls it. The explicit zone anchors a nominal head such as `Elves`.
    let explicit_battlefield = query.controller.is_none()
        && ["on", "the", "battlefield"]
            .iter()
            .enumerate()
            .all(|(i, word)| {
                cursor
                    .peek_ahead(i)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            });
    if explicit_battlefield {
        cursor.enter("numeric.for-each.battlefield");
        cursor.consume_phrase(&["on", "the", "battlefield"])?;
    }
    cursor.require(
        battlefield_anchored(&query)
            || (explicit_battlefield
                && query.predicate.supports_domain(ObjectKind::Permanent)
                && query.predicate.has_nominal_head()),
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

/// Lexical lookahead: the counted collection is followed by `in` before the
/// end of its noun phrase. The scoped query proves the actual zone grammar;
/// a collection without that relation keeps the battlefield reading.
fn counted_zone_relation_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    if recognize_optional_source_exclusion(&mut trial).unwrap_or(false) {
        return false;
    }
    recognize_object_collection(&mut trial).is_ok_and(|query| query.controller.is_none())
        && trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
}

/// A collection is read on the battlefield when a permanent card type anchors
/// it, or when a noun compatible with permanents (a subtype such as
/// `Elementals`) is qualified by a control relation: `X you control` names
/// permanents (CR 108.4, 110.2), not cards in another zone.
fn battlefield_anchored(query: &ObjectQueryNode) -> bool {
    query.predicate.supports_domain(ObjectKind::Permanent)
        && (query.predicate.has_domain(ObjectKind::Permanent)
            || (query.controller.is_some() && query.predicate.has_nominal_head()))
}

/// Lexical lookahead for `the greatest power|toughness among ...`.
pub(in crate::oracle::canonical) fn greatest_property_among_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("greatest"))
}

/// `the greatest power|toughness among COLLECTION`: a maximum aggregate over
/// the same battlefield collection the counting introducers read. The engine
/// evaluates it for the ability controller's permanents only, so any other
/// controller relation is refused explicitly.
pub(in crate::oracle::canonical) fn recognize_greatest_property_among(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.greatest-among.quantifier");
    cursor.consume_phrase(&["the", "greatest"])?;
    cursor.enter("numeric.greatest-among.property");
    let property = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("power") => NumericObjectProperty::Power,
        Some("toughness") => NumericObjectProperty::Toughness,
        _ => return Err(cursor.fail("`power` or `toughness` after `the greatest`")),
    };
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    cursor.enter("numeric.greatest-among.among");
    cursor.consume("among")?;
    cursor.enter("numeric.greatest-among.collection");
    let query = recognize_object_collection(cursor)?;
    cursor.require(
        battlefield_anchored(&query),
        "an aggregated collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "an aggregated collection predicate independent of targets and declared variables",
    )?;
    cursor.enter("numeric.greatest-among.controller");
    cursor.require(
        query.controller == Some(ObjectControllerScope::AbilityController),
        "an aggregated collection controlled by you",
    )?;
    Ok(NumericExpression::GreatestPropertyAmong {
        property,
        predicate: query.predicate,
    })
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
        LocalEffectNode::DealDamage(node) => &mut node.amount,
        LocalEffectNode::CreateExplicitTokens(node) => &mut node.quantity,
        LocalEffectNode::PlayerAction(node) => match &mut node.action {
            SingularPlayerAction::Draw(quantity) | SingularPlayerAction::Mill(quantity) => quantity,
            SingularPlayerAction::Life(life) => &mut life.amount,
            SingularPlayerAction::Discard(_)
            | SingularPlayerAction::Sacrifice(_)
            | SingularPlayerAction::Venture(_)
            | SingularPlayerAction::Proliferate
            | SingularPlayerAction::Hand(_)
            | SingularPlayerAction::ReferencedCards { .. } => return false,
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
