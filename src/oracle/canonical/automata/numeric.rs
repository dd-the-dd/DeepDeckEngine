use super::super::{MachineResult, OracleTokenCursor, parse_number_word, parse_ordinal_word};
use super::arithmetic_expression::recognize_arithmetic_expression;
use super::{
    EventNumericBinding, GraveyardScope, NumericAtom, NumericExpression, TargetBinding,
    recognize_source_object_property,
};
use super::{LocalPlayerReference, LocalZone, NumericRounding};

/// Resolves Oracle's event-bound `that many`/`that much` numeric anaphor. An
/// explicit parent binding wins; otherwise the binding the enclosing triggered
/// ability installed on the cursor applies. Without either it is a failure.
pub(in crate::oracle::canonical) fn recognize_that_many_event_value(
    cursor: &mut OracleTokenCursor<'_>,
    binding: Option<EventNumericBinding>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.event-anaphor.demonstrative");
    cursor.consume("that")?;
    cursor.enter("numeric.event-anaphor.quantity");
    let quantity = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("many") || raw.eq_ignore_ascii_case("much"))
        .ok_or_else(|| cursor.fail("many or much"))?
        .to_string();
    cursor.consume(&quantity)?;
    cursor.enter("numeric.event-anaphor.resolve");
    binding
        .or(cursor.event_numeric_binding())
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
                // Articles keep their lexical determiner role. In a numeric
                // expression this child resolves their contextual value to
                // one; it must not force the dictionary to classify all `a`
                // tokens as quantities before their parent is known.
                let article = cursor.peek_token().is_some_and(|token| {
                    token.token_type == "quantifierWord"
                        && (token.raw.eq_ignore_ascii_case("a")
                            || token.raw.eq_ignore_ascii_case("an"))
                });
                let token = if article {
                    cursor.consume_type("quantifierWord")?
                } else {
                    cursor.consume_type("quantity")?
                };
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

/// Named anaphora retain their required nominal type until the parent can
/// validate it against the original target slot. Lexical recognition alone
/// does not establish that a target is an artifact, creature, or permanent.
fn recognize_numeric_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<(TargetBinding, Option<super::ObjectPredicate>)> {
    cursor.enter("numeric.expression.object-reference");
    let reference = super::recognize_typed_target_reference(
        cursor,
        antecedent,
        "numeric.expression.object-reference.nominal",
    )?;
    Ok((
        reference.binding,
        reference
            .required_nominal
            .map(super::TargetNominal::as_predicate),
    ))
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
                if super::equal_to_quantity_family_starts(cursor) {
                    // A count, aggregate or cost-object quantity is the
                    // same `equal to` amount every action consumer reads.
                    expression = Some(super::recognize_equal_to_amount_expression(cursor, None)?);
                    NumericExpressionState::Complete
                } else {
                    NumericExpressionState::ObjectReference
                }
            }
            NumericExpressionState::ObjectReference => {
                object = Some(recognize_numeric_object_reference(cursor, antecedent)?);
                NumericExpressionState::Property
            }
            NumericExpressionState::Property => {
                cursor.enter("numeric.expression.property");
                let (binding, nominal) = cursor
                    .require_value(object.take(), "a resolved object for the numeric property")?;
                let property;
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("power"))
                {
                    cursor.consume("power")?;
                    property = super::SelectorNumericProperty::Power;
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("toughness"))
                {
                    cursor.consume("toughness")?;
                    property = super::SelectorNumericProperty::Toughness;
                } else {
                    cursor.consume("mana")?;
                    cursor.enter("numeric.expression.mana-value");
                    cursor.consume("value")?;
                    property = super::SelectorNumericProperty::ManaValue;
                }
                expression = Some(match nominal {
                    Some(required_nominal) => NumericExpression::QualifiedTargetProperty {
                        object: binding,
                        required_nominal,
                        property,
                    },
                    None => match property {
                        super::SelectorNumericProperty::Power => {
                            NumericExpression::PowerOf(binding)
                        }
                        super::SelectorNumericProperty::ManaValue => {
                            NumericExpression::ManaValueOf(binding)
                        }
                        // Keep the new property typed even without an explicit noun.
                        super::SelectorNumericProperty::Toughness => {
                            NumericExpression::QualifiedTargetProperty {
                                object: binding,
                                required_nominal: super::ObjectPredicate::ObjectKind(
                                    super::ObjectKind::Permanent,
                                ),
                                property,
                            }
                        }
                    },
                });
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

#[cfg(test)]
#[path = "numeric_qualified_target_tests.rs"]
mod qualified_target_tests;
