use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, NumericExpression, PlayerPredicate, TargetBinding,
    TargetListNode, TargetSelector, recognize_numeric_expression, recognize_target_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ScryEffectNode {
    pub player: LocalPlayerReference,
    pub count: NumericExpression,
}

/// An ordered pair of independent player effects following one target-player
/// declaration. The target remains separate so a parent envelope can give its
/// decision a context-specific id and lower the retained binding directly to
/// one chosen player for both effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetPlayerScryThenDrawNode {
    pub target: TargetListNode,
    pub target_binding: TargetBinding,
    pub scry_count: NumericExpression,
    pub draw_count: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScryState {
    Action,
    Count,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetPlayerScryThenDrawState {
    Target,
    Scry,
    Separator,
    SequenceConnector,
    DrawAction,
    DrawArticle,
    DrawCard,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScryReminderState {
    Open,
    Look,
    Cards,
    Destination,
    Close,
    Complete,
}

/// Recognizes the keyword action `scry NUMERIC_EXPRESSION` from the shared
/// immutable token stream. Punctuation and reminder text remain parent-owned.
pub(in crate::oracle::canonical) fn recognize_scry_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ScryEffectNode> {
    recognize_scry_effect_with_player(
        cursor,
        LocalPlayerReference::Controller,
        "scry",
        "the `scry` action",
    )
}

fn recognize_scry_effect_with_player(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    action: &'static str,
    expected_action: &'static str,
) -> MachineResult<ScryEffectNode> {
    let count = recognize_scry_action(cursor, action, expected_action)?;
    Ok(ScryEffectNode { player, count })
}

fn recognize_scry_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: &'static str,
    expected_action: &'static str,
) -> MachineResult<NumericExpression> {
    let mut state = ScryState::Action;
    let mut count = None;
    loop {
        state = match state {
            ScryState::Action => {
                cursor.enter("effect.scry.action");
                let found = cursor.consume_type("actionWord")?;
                cursor.require(found.raw.eq_ignore_ascii_case(action), expected_action)?;
                ScryState::Count
            }
            ScryState::Count => {
                cursor.enter("effect.scry.count");
                let expression = recognize_numeric_expression(cursor, None)?;
                cursor.require(
                    !matches!(
                        expression,
                        NumericExpression::Atom(NumericAtom::Integer(value)) if value < 0
                    ),
                    "a nonnegative scry count or variable expression",
                )?;
                count = Some(expression);
                ScryState::Complete
            }
            ScryState::Complete => {
                cursor.enter("effect.scry.complete");
                return cursor.require_value(count, "a scry count");
            }
        };
    }
}

/// Recognizes `target player/opponent scries N, then draws a card` without
/// consuming the parent-owned sentence terminator. The node retains one target
/// binding plus the two typed counts in Oracle execution order; the parent
/// supplies the same concrete chosen player while lowering both effects.
pub(in crate::oracle::canonical) fn recognize_target_player_scry_then_draw(
    cursor: &mut OracleTokenCursor<'_>,
    target_binding: TargetBinding,
) -> MachineResult<TargetPlayerScryThenDrawNode> {
    let mut state = TargetPlayerScryThenDrawState::Target;
    let mut target = None;
    let mut scry_count = None;
    let mut draw_count = None;
    loop {
        state = match state {
            TargetPlayerScryThenDrawState::Target => {
                cursor.enter("effect.target-player-scry-draw.target");
                let recognized = recognize_target_list(cursor)?;
                cursor.require(
                    recognized.slots.len() == 1
                        && recognized.slots[0].cardinality.minimum == 1
                        && recognized.slots[0].cardinality.maximum == 1
                        && matches!(
                            &recognized.slots[0].selector,
                            TargetSelector::Player(
                                PlayerPredicate::Any | PlayerPredicate::Opponent
                            )
                        ),
                    "exactly one target player or opponent",
                )?;
                target = Some(recognized);
                TargetPlayerScryThenDrawState::Scry
            }
            TargetPlayerScryThenDrawState::Scry => {
                cursor.enter("effect.target-player-scry-draw.scry");
                scry_count = Some(recognize_scry_action(
                    cursor,
                    "scries",
                    "the singular-subject `scries` action",
                )?);
                TargetPlayerScryThenDrawState::Separator
            }
            TargetPlayerScryThenDrawState::Separator => {
                cursor.enter("effect.target-player-scry-draw.separator");
                cursor.consume(",")?;
                TargetPlayerScryThenDrawState::SequenceConnector
            }
            TargetPlayerScryThenDrawState::SequenceConnector => {
                cursor.enter("effect.target-player-scry-draw.connector");
                cursor.consume("then")?;
                TargetPlayerScryThenDrawState::DrawAction
            }
            TargetPlayerScryThenDrawState::DrawAction => {
                cursor.enter("effect.target-player-scry-draw.draw-action");
                cursor.consume("draws")?;
                TargetPlayerScryThenDrawState::DrawArticle
            }
            TargetPlayerScryThenDrawState::DrawArticle => {
                cursor.enter("effect.target-player-scry-draw.draw-article");
                cursor.consume("a")?;
                TargetPlayerScryThenDrawState::DrawCard
            }
            TargetPlayerScryThenDrawState::DrawCard => {
                cursor.enter("effect.target-player-scry-draw.draw-card");
                cursor.consume("card")?;
                draw_count = Some(NumericExpression::Atom(NumericAtom::Integer(1)));
                TargetPlayerScryThenDrawState::Complete
            }
            TargetPlayerScryThenDrawState::Complete => {
                cursor.enter("effect.target-player-scry-draw.complete");
                return Ok(TargetPlayerScryThenDrawNode {
                    target: cursor.require_value(target, "one target-player declaration")?,
                    target_binding,
                    scry_count: cursor.require_value(scry_count, "one target-player scry count")?,
                    draw_count: cursor.require_value(draw_count, "one target-player draw count")?,
                });
            }
        };
    }
}

/// The library-inspection keyword actions whose reminder text is validated by
/// one shared reader. Each action supplies only its own verb and the wording
/// of where the inspected cards may go; the quantity and the reminder frame are
/// common.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LibraryLookAction {
    Scry,
    Surveil,
}

impl LibraryLookAction {
    fn verb(self) -> &'static str {
        match self {
            Self::Scry => "scry",
            Self::Surveil => "surveil",
        }
    }

    fn state(self, scry: &'static str, surveil: &'static str) -> &'static str {
        match self {
            Self::Scry => scry,
            Self::Surveil => surveil,
        }
    }
}

/// A parenthetical description belongs to a library-inspection keyword only
/// when its lexical introducer names that keyword (`To scry`, `To surveil`) or
/// starts the shared library inspection procedure (`Look at`).
pub(in crate::oracle::canonical) fn library_look_reminder_starts_at(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek() == Some("(")
        && (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("look"))
            || (cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("to"))
                && cursor.peek_ahead(2).is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("scry") || raw.eq_ignore_ascii_case("surveil")
                })))
}

/// Validates the canonical reminder used by literal scry actions. The
/// singular form is the Oracle wording on Opt; the plural form follows the
/// same grammar and verifies that its repeated quantity matches the action.
pub(in crate::oracle::canonical) fn recognize_scry_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    recognize_library_look_reminder(cursor, LibraryLookAction::Scry, count)
}

pub(in crate::oracle::canonical) fn recognize_library_look_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    action: LibraryLookAction,
    count: &NumericExpression,
) -> MachineResult<()> {
    let expected_count = match count {
        NumericExpression::Atom(NumericAtom::Integer(value)) if *value > 0 => *value,
        _ => {
            return Err(cursor.fail(action.state(
                "a literal positive scry count for reminder validation",
                "a literal positive surveil count for reminder validation",
            )));
        }
    };
    let same_count = action.state(
        "the same literal count as the scry effect",
        "the same literal count as the surveil effect",
    );
    let mut state = ScryReminderState::Open;
    loop {
        state = match state {
            ScryReminderState::Open => {
                cursor.enter(
                    action.state("effect.scry.reminder.open", "effect.surveil.reminder.open"),
                );
                cursor.consume("(")?;
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("to"))
                {
                    cursor.consume("to")?;
                    cursor.consume(action.verb())?;
                    let repeated = recognize_numeric_expression(cursor, None)?;
                    cursor.require(repeated == *count, same_count)?;
                    cursor.consume(",")?;
                }
                ScryReminderState::Look
            }
            ScryReminderState::Look => {
                cursor.enter(
                    action.state("effect.scry.reminder.look", "effect.surveil.reminder.look"),
                );
                cursor.consume_phrase(&["look", "at", "the", "top"])?;
                ScryReminderState::Cards
            }
            ScryReminderState::Cards => {
                cursor.enter(action.state(
                    "effect.scry.reminder.cards",
                    "effect.surveil.reminder.cards",
                ));
                if expected_count == 1 {
                    cursor.consume("card")?;
                } else {
                    let actual = recognize_numeric_expression(cursor, None)?;
                    cursor.require(
                        actual == NumericExpression::Atom(NumericAtom::Integer(expected_count)),
                        same_count,
                    )?;
                    cursor.consume("cards")?;
                }
                cursor.consume_phrase(&["of", "your", "library"])?;
                ScryReminderState::Destination
            }
            ScryReminderState::Destination => {
                cursor.enter(action.state(
                    "effect.scry.reminder.destination",
                    "effect.surveil.reminder.destination",
                ));
                match (action, expected_count) {
                    (LibraryLookAction::Scry, 1) => cursor.consume_phrase(&[
                        ".", "you", "may", "put", "that", "card", "on", "the", "bottom", ".",
                    ])?,
                    (LibraryLookAction::Surveil, 1) => {
                        cursor.consume_phrase(&[".", "you", "may", "put"])?;
                        // Both printed wordings name the one looked-at card:
                        // `it` (current) and `that card` (older printings).
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                        {
                            cursor.consume_phrase(&["that", "card"])?;
                        } else {
                            cursor.consume("it")?;
                        }
                        cursor.consume_phrase(&["into", "your", "graveyard", "."])?;
                    }
                    (LibraryLookAction::Scry, _) => {
                        cursor.consume_phrase(&[
                            ",", "then", "put", "any", "number", "of", "them", "on", "the",
                            "bottom",
                        ])?;
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
                        {
                            cursor.consume_phrase(&["of", "your", "library"])?;
                        }
                        cursor.consume_phrase(&[
                            "and", "the", "rest", "on", "top", "in", "any", "order", ".",
                        ])?;
                    }
                    (LibraryLookAction::Surveil, _) => cursor.consume_phrase(&[
                        ",",
                        "then",
                        "put",
                        "any",
                        "number",
                        "of",
                        "them",
                        "into",
                        "your",
                        "graveyard",
                        "and",
                        "the",
                        "rest",
                        "on",
                        "top",
                        "of",
                        "your",
                        "library",
                        "in",
                        "any",
                        "order",
                        ".",
                    ])?,
                }
                ScryReminderState::Close
            }
            ScryReminderState::Close => {
                cursor.enter(action.state(
                    "effect.scry.reminder.close",
                    "effect.surveil.reminder.close",
                ));
                cursor.consume(")")?;
                ScryReminderState::Complete
            }
            ScryReminderState::Complete => {
                cursor.enter(action.state(
                    "effect.scry.reminder.complete",
                    "effect.surveil.reminder.complete",
                ));
                return Ok(());
            }
        };
    }
}

#[cfg(test)]
#[path = "scry_tests.rs"]
mod tests;
