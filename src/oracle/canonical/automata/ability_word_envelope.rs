//! Registered ability words label a grammar; they do not change its effects.
use super::super::{MachineResult, OracleTokenCursor};
use crate::oracle::model::RecognizedEntity;

pub(in crate::oracle::canonical) fn ability_word_body_start(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    let words = tokens
        .iter()
        .take_while(|token| token.token_type == "abilityWord")
        .count();
    if words == 0 {
        return Some(0);
    }
    tokens
        .get(words)
        .filter(|token| token.raw == "\u{2014}" && token.token_type == "separator")
        .map(|_| words + 1)
}

pub(in crate::oracle::canonical) fn recognize_optional_ability_word_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    cursor.enter("ability-word.envelope.words");
    let mut found = false;
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "abilityWord")
    {
        cursor.consume_type("abilityWord")?;
        found = true;
    }
    if found {
        cursor.enter("ability-word.envelope.separator");
        cursor.consume("\u{2014}")?;
    }
    cursor.enter("ability-word.envelope.complete");
    Ok(())
}

/// Where the body of an ability starts once a rules-free label is removed
/// (CR 207.2c-d), or `None` when the ability has no such label. Saga chapter
/// symbols share the label shape but number chapter abilities (CR 714.2b), so
/// they are never stripped; Dungeon room names are claimed before this point.
pub(in crate::oracle::canonical) fn rules_free_label_body_start(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    let start =
        ability_word_body_start(tokens).filter(|start| *start > 0 && *start < tokens.len())?;
    let chapter_symbols = tokens[..start - 1]
        .iter()
        .all(|token| token.raw == "," || super::roman_chapter_value(&token.raw).is_some());
    (!chapter_symbols).then_some(start)
}

/// A labeled paragraph of a spell continues the spell's instructions (CR
/// 207.2c: the label has no rules meaning; CR 608.2c: the spell's
/// instructions are followed in written order, in one lexical scope). When
/// the paragraph alone failed because a reference in it has no antecedent
/// (`Morbid — That creature gets ...`, `Addendum — ..., put a +1/+1 counter
/// on each of those creatures`), its antecedent is in the paragraph before
/// it: the byte offset of the body after the label, which the logical
/// ability boundary appends to that paragraph. `None` for an unlabeled
/// paragraph or any other failure, which stays the paragraph's own.
pub(in crate::oracle::canonical) fn spell_paragraph_continuation(
    tokens: &[RecognizedEntity],
    failure_state: &str,
) -> Option<usize> {
    if !super::unresolved_antecedent_state(failure_state) {
        return None;
    }
    let start = rules_free_label_body_start(tokens)?;
    tokens.get(start).map(|token| token.span_start)
}

/// The label of an activated ability, read before its costs. An ability or
/// flavor word (CR 207.2c) has no rules meaning and yields `None`; an
/// activation keyword (`Boast —`, `Exhaust —`, `Forecast —`, `Power-up —`)
/// adds the rules of its definition and is returned to the activated parent.
/// Any other catalog mechanic label is left unread, so its costs reader
/// reports it explicitly instead of silently dropping its rules.
pub(in crate::oracle::canonical) fn recognize_activation_label(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<super::ActivationKeyword>> {
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    let words = tokens
        .iter()
        .take_while(|token| token.token_type == "abilityWord")
        .count();
    if words > 0
        && tokens
            .get(words)
            .is_some_and(|token| token.raw == "\u{2014}")
    {
        cursor.enter("activation.effect-sequence.label");
        cursor.advance_to(cursor.position() + words + 1)?;
        return Ok(None);
    }
    let keyword =
        crate::oracle::ability_catalog::match_catalog_mechanic(tokens, 0).and_then(|matched| {
            super::ActivationKeyword::from_canonical_id(matched.canonical_id)
                .filter(|_| {
                    tokens
                        .get(matched.end_token)
                        .is_some_and(|token| token.raw == "\u{2014}")
                })
                .map(|keyword| (keyword, matched.end_token))
        });
    if let Some((keyword, width)) = keyword {
        cursor.enter("activation.effect-sequence.keyword-label");
        cursor.advance_to(cursor.position() + width + 1)?;
        return Ok(Some(keyword));
    }
    Ok(None)
}
