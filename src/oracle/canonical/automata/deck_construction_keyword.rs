//! Partner abilities modify Commander deck construction and function before
//! the game begins (CR 702.124a). They have no in-game behavior, so they are
//! typed keywords of the object that the engine accepts without any effect.
//!
//! ```text
//! DeckKeyword := "partner" ["—" VARIANT]          CR 702.124h-i
//!              | "choose" "a" "background"        CR 702.124k
//!              | "doctor's" "companion"           CR 702.124m
//!              [ "(" REMINDER ")" ]
//! ```
//!
//! `Partner with [name]` also represents an enters trigger (CR 702.124j); it
//! is recognized as a partner ability and then fails explicitly until that
//! trigger is composed. Companion is not a partner ability: it grants a
//! special action during the game (CR 702.139a, 116.2g).
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DeckConstructionKeyword {
    /// `partner` (CR 702.124h).
    Partner,
    /// `partner—[text]` (CR 702.124i); the variant names its pairing group.
    PartnerVariant(String),
    /// `choose a Background` (CR 702.124k).
    ChooseABackground,
    /// `Doctor's companion` (CR 702.124m).
    DoctorsCompanion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeckKeywordState {
    Keyword,
    PartnerVariant,
    ReminderOrComplete,
    Reminder,
    Complete,
}

/// Normalized word: lowercase with a typographic apostrophe folded.
fn word(raw: &str) -> String {
    raw.replace('\u{2019}', "'").to_lowercase()
}

fn at_word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| word(raw) == expected)
}

fn consume_words(cursor: &mut OracleTokenCursor<'_>, words: &[&str]) -> MachineResult<()> {
    for (offset, expected) in words.iter().enumerate() {
        cursor.require(
            at_word(cursor, offset, expected),
            "the printed partner ability name",
        )?;
    }
    cursor.advance_to(cursor.position() + words.len())
}

/// Whether the ability starts with a partner ability name (CR 702.124a).
pub(in crate::oracle::canonical) fn deck_construction_keyword_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    at_word(cursor, 0, "partner")
        || (at_word(cursor, 0, "choose")
            && at_word(cursor, 1, "a")
            && at_word(cursor, 2, "background"))
        || (at_word(cursor, 0, "doctor's") && at_word(cursor, 1, "companion"))
}

/// The reminder printed for each partner ability, word for word.
fn reminder_words(keyword: &DeckConstructionKeyword) -> &'static [&'static str] {
    match keyword {
        DeckConstructionKeyword::Partner => &[
            "you",
            "can",
            "have",
            "two",
            "commanders",
            "if",
            "both",
            "have",
            "partner",
            ".",
        ],
        DeckConstructionKeyword::PartnerVariant(_) => &[
            "you",
            "can",
            "have",
            "two",
            "commanders",
            "if",
            "both",
            "have",
            "this",
            "ability",
            ".",
        ],
        DeckConstructionKeyword::ChooseABackground => &[
            "you",
            "can",
            "have",
            "a",
            "background",
            "as",
            "a",
            "second",
            "commander",
            ".",
        ],
        DeckConstructionKeyword::DoctorsCompanion => &[
            "you",
            "can",
            "have",
            "two",
            "commanders",
            "if",
            "the",
            "other",
            "is",
            "the",
            "doctor",
            ".",
        ],
    }
}

/// Recognizes one complete partner ability line.
pub(in crate::oracle::canonical) fn recognize_deck_construction_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DeckConstructionKeyword> {
    let mut state = DeckKeywordState::Keyword;
    let mut keyword = None;
    loop {
        state = match state {
            DeckKeywordState::Keyword => {
                cursor.enter("keyword.deck-construction.name");
                if at_word(cursor, 0, "partner") {
                    consume_words(cursor, &["partner"])?;
                    if at_word(cursor, 0, "with") {
                        cursor.enter("keyword.partner-with.trigger");
                        return Err(cursor.fail(
                            "a composed enters trigger for `partner with [name]` (CR 702.124j)",
                        ));
                    }
                    if cursor.peek() == Some("\u{2014}") {
                        cursor.consume("\u{2014}")?;
                        DeckKeywordState::PartnerVariant
                    } else {
                        keyword = Some(DeckConstructionKeyword::Partner);
                        DeckKeywordState::ReminderOrComplete
                    }
                } else if at_word(cursor, 0, "choose") {
                    consume_words(cursor, &["choose", "a", "background"])?;
                    keyword = Some(DeckConstructionKeyword::ChooseABackground);
                    DeckKeywordState::ReminderOrComplete
                } else {
                    consume_words(cursor, &["doctor's", "companion"])?;
                    keyword = Some(DeckConstructionKeyword::DoctorsCompanion);
                    DeckKeywordState::ReminderOrComplete
                }
            }
            DeckKeywordState::PartnerVariant => {
                // CR 702.124i: the variant is a short name such as `Friends
                // forever` or `Father & son`, never a sentence.
                cursor.enter("keyword.partner.variant");
                let start = cursor.position();
                let mut end = start;
                while cursor
                    .peek_ahead(end - start)
                    .is_some_and(|raw| !matches!(raw, "(" | ")" | "." | "," | ":" | ";" | "\""))
                {
                    end += 1;
                }
                cursor.require(
                    end > start && end - start <= 4,
                    "a short partner variant name (CR 702.124i)",
                )?;
                let name = cursor
                    .token_slice(start, end)
                    .iter()
                    .map(|token| token.raw.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                cursor.advance_to(end)?;
                keyword = Some(DeckConstructionKeyword::PartnerVariant(name));
                DeckKeywordState::ReminderOrComplete
            }
            DeckKeywordState::ReminderOrComplete => {
                cursor.enter("keyword.deck-construction.reminder-or-complete");
                match cursor.peek() {
                    None => DeckKeywordState::Complete,
                    Some("(") => DeckKeywordState::Reminder,
                    _ => return Err(cursor.fail("the partner ability reminder or end of input")),
                }
            }
            DeckKeywordState::Reminder => {
                cursor.enter("keyword.deck-construction.reminder");
                let expected = reminder_words(
                    keyword
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a recognized partner ability"))?,
                );
                cursor.consume("(")?;
                for (offset, expected_word) in expected.iter().enumerate() {
                    cursor.require(
                        at_word(cursor, offset, expected_word),
                        "the canonical reminder of this partner ability",
                    )?;
                }
                cursor.advance_to(cursor.position() + expected.len())?;
                cursor.consume(")")?;
                DeckKeywordState::Complete
            }
            DeckKeywordState::Complete => {
                cursor.enter("keyword.deck-construction.complete");
                cursor.finish()?;
                return cursor.require_value(keyword, "a recognized partner ability");
            }
        };
    }
}
