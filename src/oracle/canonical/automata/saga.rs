//! Saga chapters are a typed container (CR 715.2): the header owns the Roman
//! numeral list and the dash; the body is the shared container body slot,
//! read exactly like a triggered ability's body (CR 715.2b). No chapter body
//! is recognized here: a quoted grant (`This Saga gains "..."`), a library
//! search or any other instruction is an ordinary effect program.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, LocalObjectReference, TriggerBodyNode, recognize_container_body,
    triggered_body_context, triggered_modal_body_context,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SagaProgressionReminderNode {
    pub final_chapter: i64,
}

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct SagaChapterNode {
    pub chapters: Vec<i64>,
    /// CR 715.2b: the chapter ability's body is the shared container body.
    pub body: TriggerBodyNode,
}

pub(in crate::oracle::canonical) fn roman_chapter_value(raw: &str) -> Option<i64> {
    match raw {
        "I" => Some(1),
        "II" => Some(2),
        "III" => Some(3),
        "IV" => Some(4),
        "V" => Some(5),
        "VI" => Some(6),
        "VII" => Some(7),
        "VIII" => Some(8),
        "IX" => Some(9),
        "X" => Some(10),
        _ => None,
    }
}

fn recognize_roman_chapter(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    cursor.enter("saga.chapter.roman");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a Saga chapter numeral from I through X"))?
        .to_string();
    let value = roman_chapter_value(&raw)
        .ok_or_else(|| cursor.fail("a Saga chapter numeral from I through X"))?;
    cursor.consume(&raw)?;
    Ok(value)
}

/// Recognizes the standardized Saga progression reminder as typed syntax.
/// The marker documents the parsed final chapter; it does not replace the
/// runtime's chapter-event rules.
pub(in crate::oracle::canonical) fn recognize_saga_progression_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SagaProgressionReminderNode> {
    cursor.enter("saga.reminder.open");
    cursor.consume("(")?;
    cursor.enter("saga.reminder.progression");
    cursor.consume_phrase(&[
        "as", "this", "Saga", "enters", "and", "after", "your", "draw", "step", ",", "add", "a",
        "lore", "counter", ".",
    ])?;
    cursor.enter("saga.reminder.sacrifice");
    cursor.consume_phrase(&["sacrifice", "after"])?;
    let final_chapter = recognize_roman_chapter(cursor)?;
    cursor.enter("saga.reminder.close");
    cursor.consume_phrase(&[".", ")"])?;
    cursor.finish()?;
    Ok(SagaProgressionReminderNode { final_chapter })
}

/// Recognizes the Saga chapter header and commits it before delegating the
/// body to the shared container body slot.
pub(in crate::oracle::canonical) fn recognize_saga_chapter(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SagaChapterNode> {
    cursor.enter("saga.chapter.header");
    let mut chapters = vec![recognize_roman_chapter(cursor)?];
    while cursor.peek().is_some_and(|raw| raw == ",") {
        cursor.enter("saga.chapter.separator");
        cursor.consume(",")?;
        chapters.push(recognize_roman_chapter(cursor)?);
    }
    cursor.require(
        chapters.windows(2).all(|pair| pair[0] < pair[1]),
        "strictly increasing, nonduplicated Saga chapter numerals",
    )?;
    cursor.enter("saga.chapter.dash");
    let dash = cursor
        .peek()
        .filter(|raw| matches!(*raw, "—" | "-"))
        .ok_or_else(|| cursor.fail("an em dash after the Saga chapter list"))?
        .to_string();
    cursor.consume(&dash)?;

    cursor.enter("saga.chapter.body");
    let source = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
    let program_context = triggered_body_context(source.clone(), None);
    let modal_context = triggered_modal_body_context(source, &program_context);
    let body = recognize_container_body(
        cursor,
        name_links,
        name_failure,
        &program_context,
        &modal_context,
    )?;
    cursor.enter("saga.chapter.complete");
    cursor.finish()?;
    Ok(SagaChapterNode { chapters, body })
}

#[cfg(test)]
#[path = "saga_tests.rs"]
mod tests;
