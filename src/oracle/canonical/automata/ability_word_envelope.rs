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
