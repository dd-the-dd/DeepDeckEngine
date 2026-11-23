use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AbilityRestrictionOwner, ActivatedManaAbilityNode, CharacteristicTokenActivationNode,
    EffectProgramNode, EffectRecognitionContext, LibrarySearchEffectNode, LocalObjectReference,
    ZoneChangeDomains, recognize_activated_mana_ability, recognize_characteristic_token_activation,
    recognize_controller_library_search, recognize_effect_program_with_context,
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

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum SagaChapterBodyNode {
    Grant(SagaGrantedAbilityNode),
    LibrarySearch(LibrarySearchEffectNode),
    /// CR 714.2b: any other chapter ability is an ordinary triggered
    /// ability whose effect is the shared effect program.
    Program(EffectProgramNode),
}

#[derive(Clone, Debug)]
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
    // The quoted grant is a structural form (`this Saga gains "..."`): the
    // quote opens its own ability, so it is claimed only with its quote.
    let grant = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        && cursor.peek_ahead(1).is_some_and(|raw| raw == "Saga")
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("gains"))
        && cursor.peek_ahead(3) == Some("\"");
    let body = if grant {
        SagaChapterBodyNode::Grant(recognize_granted_ability(cursor, name_links, name_failure)?)
    } else if let Ok(search) = cursor.attempt(|attempt| {
        let search = recognize_controller_library_search(attempt)?;
        // The native child lowers only the controller's search onto the
        // battlefield; any other search is an ordinary program.
        attempt.require(
            search.player == super::LocalPlayerReference::Controller
                && matches!(
                    search.destination,
                    super::SearchDestination::Battlefield { .. }
                ),
            "the controller's library search onto the battlefield",
        )?;
        attempt.enter("saga.chapter.search.terminator");
        attempt.consume(".")?;
        attempt.finish()?;
        Ok(search)
    }) {
        SagaChapterBodyNode::LibrarySearch(search)
    } else {
        cursor.enter("saga.chapter.program");
        SagaChapterBodyNode::Program(recognize_effect_program_with_context(
            cursor,
            name_links,
            name_failure,
            &saga_chapter_body_context(),
        )?)
    };
    cursor.enter("saga.chapter.complete");
    cursor.finish()?;
    Ok(SagaChapterNode { chapters, body })
}

/// A chapter ability triggers on its Saga permanent (CR 714.2b): the body
/// reads the same context as any other battlefield-source triggered ability
/// whose event names no other object. `this Saga` is the source; no event
/// subject is bound, so `it` keeps its explicit failure.
fn saga_chapter_body_context() -> EffectRecognitionContext {
    let mut context = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    context.ability_restriction_owner = AbilityRestrictionOwner::Triggered;
    context
}

#[cfg(test)]
#[path = "saga_tests.rs"]
mod tests;
