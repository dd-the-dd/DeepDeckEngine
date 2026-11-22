use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivatedManaAbilityNode, CharacteristicTokenActivationNode, LibrarySearchEffectNode,
    recognize_activated_mana_ability, recognize_characteristic_token_activation,
    recognize_controller_library_search,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SagaProgressionReminderNode {
    pub final_chapter: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SagaGrantedAbilityNode {
    Mana(ActivatedManaAbilityNode),
    CharacteristicToken(CharacteristicTokenActivationNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SagaChapterBodyNode {
    Grant(SagaGrantedAbilityNode),
    LibrarySearch(LibrarySearchEffectNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SagaChapterNode {
    pub chapters: Vec<i64>,
    pub body: SagaChapterBodyNode,
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

fn recognize_granted_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SagaGrantedAbilityNode> {
    cursor.enter("saga.chapter.grant.reference");
    cursor.consume_phrase(&["this", "Saga", "gains"])?;
    cursor.enter("saga.chapter.grant.open-quote");
    cursor.consume("\"")?;

    let inner_start = cursor.position();
    let mut inner_length = 0;
    while cursor
        .peek_ahead(inner_length)
        .is_some_and(|raw| raw != "\"")
    {
        inner_length += 1;
    }
    cursor.require(
        cursor.peek_ahead(inner_length) == Some("\""),
        "a closing quote for the granted ability",
    )?;
    let inner_end = inner_start + inner_length;
    let inner_tokens = cursor.token_slice(inner_start, inner_end);
    let separator = inner_tokens.iter().position(|token| token.raw == ":");
    let effect_head = separator
        .and_then(|index| inner_tokens.get(index + 1))
        .map(|token| token.raw.as_str());
    let mut nested = OracleTokenCursor::new("saga-granted-ability", inner_tokens);
    let ability = match effect_head {
        Some(raw) if raw.eq_ignore_ascii_case("add") => {
            let ability = recognize_activated_mana_ability(&mut nested, name_links, name_failure)?;
            nested.require(
                ability.followup.is_none(),
                "one atomic mana-production effect in the granted ability",
            )?;
            SagaGrantedAbilityNode::Mana(ability)
        }
        Some(raw) if is_verb_form(&raw, VerbLemma::Create, VerbForm::Base) => {
            SagaGrantedAbilityNode::CharacteristicToken(recognize_characteristic_token_activation(
                &mut nested,
            )?)
        }
        _ => {
            return Err(
                cursor.fail("a native quoted mana or characteristic-token activated ability")
            );
        }
    };

    cursor.advance_to(inner_end)?;
    cursor.enter("saga.chapter.grant.close-quote");
    cursor.consume("\"")?;
    Ok(ability)
}

/// Recognizes the shared Saga chapter envelope and delegates its body to
/// independent token-driven child automata on slices of the original stream.
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

    cursor.enter("saga.chapter.body-dispatch");
    let body = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        SagaChapterBodyNode::Grant(recognize_granted_ability(cursor, name_links, name_failure)?)
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("search"))
    {
        let search = recognize_controller_library_search(cursor)?;
        cursor.enter("saga.chapter.search.terminator");
        cursor.consume(".")?;
        SagaChapterBodyNode::LibrarySearch(search)
    } else {
        return Err(cursor.fail("a native Saga chapter body"));
    };
    cursor.enter("saga.chapter.complete");
    cursor.finish()?;
    Ok(SagaChapterNode { chapters, body })
}

#[cfg(test)]
#[path = "saga_tests.rs"]
mod tests;
