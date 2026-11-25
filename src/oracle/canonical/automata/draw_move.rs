use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionOrder, EffectRecognitionContext, LibraryPosition, LocalPlayerReference, LocalZone,
    NumericAtom, NumericExpression, recognize_card_noun, recognize_collection_order,
    recognize_controller_zone, recognize_library_position, recognize_positive_card_count,
    recognize_source_cast_x_numeric_expression, recognize_that_many_event_value,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DrawCardsEffectNode {
    pub player: LocalPlayerReference,
    pub count: NumericExpression,
}

pub(in crate::oracle::canonical) enum DrawCardsEffectParseAttempt {
    Parsed(DrawCardsEffectNode),
    RejectedCandidate(super::super::MachineFailure),
    FailedCommitted(super::super::MachineFailure),
}

enum DrawCardsEffectParseFailure {
    UnsupportedGrammar(super::super::MachineFailure),
    Malformed(super::super::MachineFailure),
}

impl From<super::super::MachineFailure> for DrawCardsEffectParseFailure {
    fn from(failure: super::super::MachineFailure) -> Self {
        Self::Malformed(failure)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MoveCardsEffectNode {
    pub player: LocalPlayerReference,
    pub count: NumericAtom,
    pub source: LocalZone,
    pub destination: LocalZone,
    pub position: LibraryPosition,
    pub order: CollectionOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawState {
    Action,
    Count,
    Cards,
    RoundingSeparator,
    RoundingMarker,
    RoundingDirection,
    Complete,
}

#[derive(Default)]
struct DrawMemory {
    count: Option<NumericExpression>,
}

/// Recognizes the independent `draw N cards` effect for the shared effect
/// sequence automaton.
pub(in crate::oracle::canonical) fn recognize_draw_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DrawCardsEffectNode> {
    recognize_draw_cards_effect_with_context(cursor, &EffectRecognitionContext::default())
}

pub(in crate::oracle::canonical) fn recognize_draw_cards_effect_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<DrawCardsEffectNode> {
    recognize_draw_cards_effect_attempt(cursor, context).map_err(|failure| match failure {
        DrawCardsEffectParseFailure::UnsupportedGrammar(failure)
        | DrawCardsEffectParseFailure::Malformed(failure) => failure,
    })
}

pub(in crate::oracle::canonical) fn try_recognize_draw_cards_effect_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> DrawCardsEffectParseAttempt {
    match recognize_draw_cards_effect_attempt(cursor, context) {
        Ok(effect) => DrawCardsEffectParseAttempt::Parsed(effect),
        Err(DrawCardsEffectParseFailure::UnsupportedGrammar(failure)) => {
            DrawCardsEffectParseAttempt::RejectedCandidate(failure)
        }
        Err(DrawCardsEffectParseFailure::Malformed(failure)) => {
            DrawCardsEffectParseAttempt::FailedCommitted(failure)
        }
    }
}

fn recognize_draw_cards_effect_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> Result<DrawCardsEffectNode, DrawCardsEffectParseFailure> {
    cursor.enter("effect.draw-cards.subject");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        cursor.consume("you")?;
    }

    recognize_draw_action_attempt(cursor, super::VerbAgreement::Base, context)
}

/// Subject-free draw child; the caller resolves actor identity and agreement.
pub(in crate::oracle::canonical) fn recognize_draw_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: super::VerbAgreement,
    context: &EffectRecognitionContext,
) -> MachineResult<NumericExpression> {
    recognize_draw_action_attempt(cursor, form, context)
        .map(|node| node.count)
        .map_err(|failure| match failure {
            DrawCardsEffectParseFailure::UnsupportedGrammar(failure)
            | DrawCardsEffectParseFailure::Malformed(failure) => failure,
        })
}

fn recognize_draw_action_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    form: super::VerbAgreement,
    context: &EffectRecognitionContext,
) -> Result<DrawCardsEffectNode, DrawCardsEffectParseFailure> {
    let verb = match form {
        super::VerbAgreement::Base => "draw",
        super::VerbAgreement::ThirdPersonSingular => "draws",
    };
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
    {
        cursor.enter("effect.draw-cards.equal-expression.action");
        cursor.consume_phrase(&[verb, "cards", "equal", "to"])?;
        cursor.enter("effect.draw-cards.equal-expression.count");
        // The count is the shared `equal to` quantity; each numeric family
        // owns its own failure and the consumer's lowering its capability.
        // A malformed implemented half-library count stays committed; any
        // other unsupported family remains a rejected candidate.
        let half = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("half"));
        let count =
            super::recognize_equal_to_amount_expression(cursor, None).map_err(|failure| {
                if half {
                    DrawCardsEffectParseFailure::Malformed(failure)
                } else {
                    DrawCardsEffectParseFailure::UnsupportedGrammar(failure)
                }
            })?;
        cursor.enter("effect.draw-cards.equal-expression.complete");
        return Ok(DrawCardsEffectNode {
            player: LocalPlayerReference::Controller,
            count,
        });
    }

    let mut state = DrawState::Action;
    let mut memory = DrawMemory::default();
    loop {
        state = match state {
            DrawState::Action => {
                cursor.enter("effect.draw-cards.action");
                cursor.consume(verb)?;
                DrawState::Count
            }
            DrawState::Count => {
                cursor.enter("effect.draw-cards.count");
                memory.count = Some(
                    if context.source_cast_x
                        && cursor.peek().is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("x") || raw.eq_ignore_ascii_case("half")
                        })
                    {
                        recognize_source_cast_x_numeric_expression(cursor)?
                    } else if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                    {
                        // `that many`: the quantity bound by the triggering event.
                        recognize_that_many_event_value(cursor, None)?
                    } else {
                        NumericExpression::Atom(recognize_positive_card_count(cursor)?)
                    },
                );
                DrawState::Cards
            }
            DrawState::Cards => {
                cursor.enter("effect.draw-cards.cards");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("additional"))
                {
                    cursor.enter("effect.draw-cards.unsupported-qualifier");
                    return Err(DrawCardsEffectParseFailure::UnsupportedGrammar(
                        cursor.fail("a supported draw-card qualifier"),
                    ));
                }
                match memory
                    .count
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated draw count"))?
                {
                    NumericExpression::Atom(atom) => recognize_card_noun(cursor, atom)?,
                    NumericExpression::SourceCastX
                    | NumericExpression::ColorsOfManaSpentToCastSource
                    | NumericExpression::ObjectProperty { .. }
                    | NumericExpression::EventValue(_)
                    | NumericExpression::CountDistinctCardTypes { .. }
                    | NumericExpression::OpponentCount(_)
                    | NumericExpression::EventHistoryCount(_)
                    | NumericExpression::Devotion(_)
                    | NumericExpression::ActionResultQuantity { .. }
                    | NumericExpression::Add { .. }
                    | NumericExpression::Subtract { .. }
                    | NumericExpression::Multiply { .. }
                    | NumericExpression::PlayerFact { .. }
                    | NumericExpression::SacrificedCostObjectProperty(_)
                    | NumericExpression::BoundObjectProperty { .. }
                    | NumericExpression::GreatestPropertyAmong { .. }
                    | NumericExpression::CardsInZone { .. }
                    | NumericExpression::Divide { .. } => {
                        cursor.consume("cards")?;
                    }
                    NumericExpression::CountQuery(_)
                    | NumericExpression::PermanentCount(_)
                    | NumericExpression::ActionResultCount { .. } => {
                        return Err(cursor.fail("a proved query-count consumer for draw").into());
                    }
                    NumericExpression::CountCounters { .. }
                    | NumericExpression::CountersOnRecipient { .. } => {
                        return Err(cursor
                            .fail("a proved source-counter information context for draw")
                            .into());
                    }
                    NumericExpression::PowerOf(_)
                    | NumericExpression::ManaValueOf(_)
                    | NumericExpression::QualifiedTargetProperty { .. } => {
                        return Err(cursor.fail("a target-independent draw count").into());
                    }
                }
                if matches!(
                    memory.count.as_ref(),
                    Some(NumericExpression::Divide { .. })
                ) {
                    DrawState::RoundingSeparator
                } else {
                    DrawState::Complete
                }
            }
            DrawState::RoundingSeparator => {
                cursor.enter("effect.draw-cards.rounding-separator");
                cursor.consume(",")?;
                DrawState::RoundingMarker
            }
            DrawState::RoundingMarker => {
                cursor.enter("effect.draw-cards.rounding-marker");
                let marker = cursor.consume_type("roundingWord")?;
                cursor.require(
                    marker.raw.eq_ignore_ascii_case("rounded"),
                    "the `rounded` marker",
                )?;
                DrawState::RoundingDirection
            }
            DrawState::RoundingDirection => {
                cursor.enter("effect.draw-cards.rounding-direction");
                let direction = cursor.consume_type("directionWord")?;
                cursor.require(
                    direction.raw.eq_ignore_ascii_case("down"),
                    "the `down` rounding direction",
                )?;
                DrawState::Complete
            }
            DrawState::Complete => {
                cursor.enter("effect.draw-cards.complete");
                return Ok(DrawCardsEffectNode {
                    player: LocalPlayerReference::Controller,
                    count: cursor.require_value(memory.count, "a populated draw count")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveState {
    Action,
    Count,
    Cards,
    SourceRelation,
    Source,
    DestinationPosition,
    Destination,
    Order,
    Complete,
}

#[derive(Default)]
struct MoveMemory {
    count: Option<NumericAtom>,
    source: Option<LocalZone>,
    destination: Option<LocalZone>,
    position: Option<LibraryPosition>,
    order: Option<CollectionOrder>,
}

/// Recognizes a selection-backed `put N cards from ZONE ...` effect. It emits
/// a typed move description; primitive choice, ordering, and movement remain
/// separate in IR lowering.
pub(in crate::oracle::canonical) fn recognize_move_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<MoveCardsEffectNode> {
    let mut state = MoveState::Action;
    let mut memory = MoveMemory::default();
    loop {
        state = match state {
            MoveState::Action => {
                cursor.enter("effect.move-cards.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                MoveState::Count
            }
            MoveState::Count => {
                cursor.enter("effect.move-cards.count");
                memory.count = Some(recognize_positive_card_count(cursor)?);
                MoveState::Cards
            }
            MoveState::Cards => {
                cursor.enter("effect.move-cards.cards");
                recognize_card_noun(
                    cursor,
                    memory
                        .count
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a populated move count"))?,
                )?;
                MoveState::SourceRelation
            }
            MoveState::SourceRelation => {
                cursor.enter("effect.move-cards.source-relation");
                cursor.consume("from")?;
                MoveState::Source
            }
            MoveState::Source => {
                cursor.enter("effect.move-cards.source");
                memory.source = Some(recognize_controller_zone(cursor)?);
                MoveState::DestinationPosition
            }
            MoveState::DestinationPosition => {
                cursor.enter("effect.move-cards.destination-position");
                memory.position = Some(recognize_library_position(cursor)?);
                MoveState::Destination
            }
            MoveState::Destination => {
                cursor.enter("effect.move-cards.destination");
                memory.destination = Some(recognize_controller_zone(cursor)?);
                MoveState::Order
            }
            MoveState::Order => {
                cursor.enter("effect.move-cards.order");
                memory.order = Some(recognize_collection_order(cursor)?);
                MoveState::Complete
            }
            MoveState::Complete => {
                cursor.enter("effect.move-cards.complete");
                return Ok(MoveCardsEffectNode {
                    player: LocalPlayerReference::Controller,
                    count: cursor.require_value(memory.count, "a populated move count")?,
                    source: cursor.require_value(memory.source, "a populated source zone")?,
                    destination: cursor
                        .require_value(memory.destination, "a populated destination zone")?,
                    position: cursor
                        .require_value(memory.position, "a populated library position")?,
                    order: cursor.require_value(memory.order, "a populated collection order")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "draw_move_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "draw_equal_support_tests.rs"]
mod draw_equal_support_tests;
