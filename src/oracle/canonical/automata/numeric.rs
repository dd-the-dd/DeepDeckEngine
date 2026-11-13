use super::super::{MachineResult, OracleTokenCursor, parse_number_word, parse_ordinal_word};
use super::arithmetic_expression::recognize_arithmetic_expression;
use super::{
    EventNumericBinding, GraveyardScope, NumericAtom, NumericExpression, TargetBinding,
    recognize_anaphoric_target_reference, recognize_source_object_property,
};
use super::{LocalPlayerReference, LocalZone, NumericRounding};

/// Resolves Oracle's event-bound `that many` numeric anaphor. The parent
/// supplies the event binding; without it the phrase is a committed failure.
pub(in crate::oracle::canonical) fn recognize_that_many_event_value(
    cursor: &mut OracleTokenCursor<'_>,
    binding: Option<EventNumericBinding>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.event-anaphor.demonstrative");
    cursor.consume("that")?;
    cursor.enter("numeric.event-anaphor.quantity");
    cursor.consume("many")?;
    cursor.enter("numeric.event-anaphor.resolve");
    binding
        .map(NumericExpression::EventValue)
        .ok_or_else(|| cursor.fail("a numeric value bound by the triggering event"))
}

/// Recognizes the reusable quantity `the number of card types among cards in
/// ZONE`. Zone ownership remains typed so the lowering layer can distinguish
/// the controller's graveyard from the union of all graveyards.
pub(in crate::oracle::canonical) fn recognize_distinct_card_type_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.distinct-card-types.quantity");
    cursor.consume_phrase(&["the", "number", "of", "card", "types"])?;
    cursor.enter("numeric.distinct-card-types.collection");
    cursor.consume_phrase(&["among", "cards", "in"])?;
    cursor.enter("numeric.distinct-card-types.zone-scope");
    let zone = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume_phrase(&["your", "graveyard"])?;
        GraveyardScope::Controller
    } else {
        cursor.consume_phrase(&["all", "graveyards"])?;
        GraveyardScope::AllPlayers
    };
    cursor.enter("numeric.distinct-card-types.complete");
    Ok(NumericExpression::CountDistinctCardTypes { zone })
}

/// Resolves `that number plus N` against a numeric expression retained by the
/// parent. The antecedent is cloned into an arithmetic AST; no source text is
/// reconstructed and an unresolved `that number` never degrades to a literal.
pub(in crate::oracle::canonical) fn recognize_that_number_plus_literal(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&NumericExpression>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.prior-number-anaphor.demonstrative");
    cursor.consume_phrase(&["that", "number"])?;
    cursor.enter("numeric.prior-number-anaphor.resolve");
    let left = antecedent
        .cloned()
        .ok_or_else(|| cursor.fail("a prior numeric-expression binding for `that number`"))?;
    cursor.enter("numeric.add.operator");
    cursor.consume("plus")?;
    cursor.enter("numeric.add.right");
    let right = NumericExpression::Atom(recognize_numeric_atom(cursor)?);
    cursor.enter("numeric.add.complete");
    Ok(NumericExpression::add(left, right))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericState {
    Start,
    Literal,
    Variable,
    Complete,
}

/// Recognizes one atomic numeric expression without rebuilding source text.
/// Compound arithmetic can call this machine for each operand later.
pub(in crate::oracle::canonical) fn recognize_numeric_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericAtom> {
    let mut state = NumericState::Start;
    let mut value = None;
    loop {
        state = match state {
            NumericState::Start => {
                cursor.enter("numeric.atom.start");
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("integer, number word, or X"))?;
                if token.raw.eq_ignore_ascii_case("x") {
                    NumericState::Variable
                } else if parse_number_word(&token.raw).is_some() {
                    NumericState::Literal
                } else {
                    return Err(cursor.fail("integer, number word, or X"));
                }
            }
            NumericState::Literal => {
                cursor.enter("numeric.atom.literal");
                let token = cursor.consume_type("quantity")?;
                value = Some(NumericAtom::Integer(
                    parse_number_word(&token.raw)
                        .ok_or_else(|| cursor.fail("a supported literal quantity"))?,
                ));
                NumericState::Complete
            }
            NumericState::Variable => {
                cursor.enter("numeric.atom.variable");
                cursor.consume("x")?;
                value = Some(NumericAtom::VariableX);
                NumericState::Complete
            }
            NumericState::Complete => {
                cursor.enter("numeric.atom.complete");
                return cursor.require_value(value, "a populated numeric atom");
            }
        };
    }
}

/// Recognizes one ordinal literal as a reusable numeric child. Sequence
/// constraints (for example, consecutive turns) remain the responsibility of
/// the parent that owns the surrounding list.
pub(in crate::oracle::canonical) fn recognize_ordinal_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<i64> {
    cursor.enter("numeric.ordinal.literal");
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("an ordinal literal"))?;
    let value = parse_ordinal_word(&token.raw).ok_or_else(|| cursor.fail("an ordinal literal"))?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericExpressionState {
    Start,
    Equality,
    ObjectReference,
    Property,
    Complete,
}

fn recognize_numeric_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<TargetBinding> {
    cursor.enter("numeric.expression.object-reference");
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("its"))
    {
        return recognize_anaphoric_target_reference(cursor, antecedent);
    }
    cursor.consume("that")?;
    cursor.enter("numeric.expression.object-reference.card");
    let object = cursor.consume_type("possessiveTypeWord")?;
    cursor.require(
        object.raw.eq_ignore_ascii_case("card's"),
        "the possessive card reference",
    )?;
    antecedent.ok_or_else(|| cursor.fail("a prior target binding for `that card's`"))
}

/// Recognizes one leaf of the shared arithmetic grammar without reconstructing
/// text. Object anaphora must resolve through parent memory.
pub(in crate::oracle::canonical) fn recognize_numeric_primary_expression(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    let mut state = NumericExpressionState::Start;
    let mut object = None;
    let mut expression = None;
    loop {
        state = match state {
            NumericExpressionState::Start => {
                cursor.enter("numeric.expression.start");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("this"))
                {
                    expression = Some(recognize_source_object_property(cursor)?);
                    NumericExpressionState::Complete
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("equal"))
                {
                    NumericExpressionState::Equality
                } else {
                    expression = Some(NumericExpression::Atom(recognize_numeric_atom(cursor)?));
                    NumericExpressionState::Complete
                }
            }
            NumericExpressionState::Equality => {
                cursor.enter("numeric.expression.equality");
                cursor.consume_phrase(&["equal", "to"])?;
                NumericExpressionState::ObjectReference
            }
            NumericExpressionState::ObjectReference => {
                object = Some(recognize_numeric_object_reference(cursor, antecedent)?);
                NumericExpressionState::Property
            }
            NumericExpressionState::Property => {
                cursor.enter("numeric.expression.property");
                let binding =
                    cursor.require_value(object, "a resolved object for the numeric property")?;
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("power"))
                {
                    cursor.consume("power")?;
                    expression = Some(NumericExpression::PowerOf(binding));
                } else {
                    cursor.consume("mana")?;
                    cursor.enter("numeric.expression.mana-value");
                    cursor.consume("value")?;
                    expression = Some(NumericExpression::ManaValueOf(binding));
                }
                NumericExpressionState::Complete
            }
            NumericExpressionState::Complete => {
                cursor.enter("numeric.expression.complete");
                return cursor.require_value(expression, "a populated numeric expression");
            }
        };
    }
}

/// Recognizes one numeric expression through the shared precedence-aware
/// arithmetic machine. Its primary leaves retain the existing typed numeric
/// atoms and object-property references.
pub(in crate::oracle::canonical) fn recognize_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_expression(cursor, antecedent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceCastXExpressionState {
    Start,
    Half,
    OptionalOf,
    Variable,
    Complete,
}

/// Recognizes the value of X retained from casting the current source, either
/// directly or through Oracle's `half [of] X` arithmetic prefix. The parent
/// owns the following noun and any explicit `rounded down` confirmation.
pub(in crate::oracle::canonical) fn recognize_source_cast_x_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let mut state = SourceCastXExpressionState::Start;
    let mut halved = false;
    loop {
        state = match state {
            SourceCastXExpressionState::Start => {
                cursor.enter("numeric.source-cast-x.start");
                match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("half") => {
                        SourceCastXExpressionState::Half
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("x") => {
                        SourceCastXExpressionState::Variable
                    }
                    _ => return Err(cursor.fail("X or `half X` from the source's cast value")),
                }
            }
            SourceCastXExpressionState::Half => {
                cursor.enter("numeric.source-cast-x.half");
                let token = cursor.consume_type("numericOperatorWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("half"),
                    "the `half` operator",
                )?;
                halved = true;
                SourceCastXExpressionState::OptionalOf
            }
            SourceCastXExpressionState::OptionalOf => {
                cursor.enter("numeric.source-cast-x.optional-of");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
                {
                    cursor.consume("of")?;
                }
                SourceCastXExpressionState::Variable
            }
            SourceCastXExpressionState::Variable => {
                cursor.enter("numeric.source-cast-x.variable");
                let token_index = cursor.position();
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("the source cast X value"))?;
                if token.token_type != "quantity" || !token.raw.eq_ignore_ascii_case("x") {
                    return Err(cursor.fail_with_span(
                        "the source cast X value",
                        token_index,
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    ));
                }
                cursor.advance_to(token_index + 1)?;
                SourceCastXExpressionState::Complete
            }
            SourceCastXExpressionState::Complete => {
                cursor.enter("numeric.source-cast-x.complete");
                let expression = NumericExpression::SourceCastX;
                return if halved {
                    NumericExpression::divide_rounded_down(expression, 2)
                        .ok_or_else(|| cursor.fail("a positive literal divisor"))
                } else {
                    Ok(expression)
                };
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LibraryCountState {
    Half,
    Determiner,
    QuantityNoun,
    Of,
    Cards,
    In,
    Possessor,
    Zone,
    Separator,
    Rounded,
    Direction,
    Complete,
}

/// Recognizes Oracle's reusable rounded half-count over the controller's
/// library without reconstructing a numeric-expression string.
pub(in crate::oracle::canonical) fn recognize_half_controller_library_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let mut state = LibraryCountState::Half;
    let mut rounding = None;
    loop {
        state = match state {
            LibraryCountState::Half => {
                cursor.enter("numeric.library-count.half");
                cursor.consume("half")?;
                LibraryCountState::Determiner
            }
            LibraryCountState::Determiner => {
                cursor.enter("numeric.library-count.determiner");
                cursor.consume("the")?;
                LibraryCountState::QuantityNoun
            }
            LibraryCountState::QuantityNoun => {
                cursor.enter("numeric.library-count.quantity-noun");
                cursor.consume("number")?;
                LibraryCountState::Of
            }
            LibraryCountState::Of => {
                cursor.enter("numeric.library-count.of");
                cursor.consume("of")?;
                LibraryCountState::Cards
            }
            LibraryCountState::Cards => {
                cursor.enter("numeric.library-count.cards");
                cursor.consume("cards")?;
                LibraryCountState::In
            }
            LibraryCountState::In => {
                cursor.enter("numeric.library-count.in");
                cursor.consume("in")?;
                LibraryCountState::Possessor
            }
            LibraryCountState::Possessor => {
                cursor.enter("numeric.library-count.possessor");
                cursor.consume("your")?;
                LibraryCountState::Zone
            }
            LibraryCountState::Zone => {
                cursor.enter("numeric.library-count.zone");
                cursor.consume("library")?;
                LibraryCountState::Separator
            }
            LibraryCountState::Separator => {
                cursor.enter("numeric.library-count.rounding-separator");
                cursor.consume(",")?;
                LibraryCountState::Rounded
            }
            LibraryCountState::Rounded => {
                cursor.enter("numeric.library-count.rounded");
                cursor.consume("rounded")?;
                LibraryCountState::Direction
            }
            LibraryCountState::Direction => {
                cursor.enter("numeric.library-count.direction");
                rounding = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("up") => {
                            cursor.consume("up")?;
                            NumericRounding::Up
                        }
                        Some("down") => {
                            cursor.consume("down")?;
                            NumericRounding::Down
                        }
                        _ => return Err(cursor.fail("`up` or `down` rounding direction")),
                    },
                );
                LibraryCountState::Complete
            }
            LibraryCountState::Complete => {
                cursor.enter("numeric.library-count.complete");
                let count = NumericExpression::CardsInZone {
                    player: LocalPlayerReference::Controller,
                    zone: LocalZone::Library,
                };
                return NumericExpression::divide(
                    count,
                    2,
                    cursor.require_value(rounding, "an explicit rounding direction")?,
                )
                .ok_or_else(|| cursor.fail("a positive literal divisor"));
            }
        };
    }
}

#[cfg(test)]
#[path = "numeric_expression_tests.rs"]
mod expression_tests;
