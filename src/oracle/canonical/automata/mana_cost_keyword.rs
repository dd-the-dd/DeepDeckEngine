//! Keyword abilities parameterized by one mana cost whose rules the engine
//! executes from that cost (CR 702.37 morph and megamorph, 702.109 dash,
//! 702.168 disguise, 702.170 plot, 702.185 warp):
//!
//! ```text
//! Atom     := Keyword ManaCost ["(" Reminder ")"]
//! Reminder := the keyword's registered reminder sentence(s), whose printed
//!             costs must equal the atom's own cost
//! ```
//!
//! The cost is the shared mana-cost child. A reminder is never skipped as
//! arbitrary text: each keyword has a typed schema (fixed tokens, the source
//! pronoun or permanent type, and the printed or rules-fixed cost).
use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, recognize_mana_cost};
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaCostKeyword {
    Morph,
    /// Morph whose permanent gets a +1/+1 counter as it is turned face up.
    Megamorph,
    /// Morph whose face-down creature has ward {2} (CR 702.168a).
    Disguise,
    Dash,
    Plot,
    Warp,
}

impl ManaCostKeyword {
    fn from_word(raw: &str) -> Option<Self> {
        Some(match raw.to_ascii_lowercase().as_str() {
            "morph" => Self::Morph,
            "megamorph" => Self::Megamorph,
            "disguise" => Self::Disguise,
            "dash" => Self::Dash,
            "plot" => Self::Plot,
            "warp" => Self::Warp,
            _ => return None,
        })
    }

    fn word(self) -> &'static str {
        match self {
            Self::Morph => "morph",
            Self::Megamorph => "megamorph",
            Self::Disguise => "disguise",
            Self::Dash => "dash",
            Self::Plot => "plot",
            Self::Warp => "warp",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaCostKeywordNode {
    pub keyword: ManaCostKeyword,
    pub cost: ManaCostNode,
}

/// A registered mana-cost keyword followed by a mana symbol.
pub(in crate::oracle::canonical) fn mana_cost_keyword_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().and_then(ManaCostKeyword::from_word).is_some()
        && cursor.peek_ahead(1).is_some_and(|raw| raw.starts_with('{'))
}

fn consume_words(cursor: &mut OracleTokenCursor<'_>, words: &[&str]) -> MachineResult<()> {
    for word in words {
        cursor.consume(word)?;
    }
    Ok(())
}

/// `it`/`her`/`him` and `its`/`her`/`his`: the source's own pronoun pair.
fn consume_source_pronoun(
    cursor: &mut OracleTokenCursor<'_>,
    possessive: bool,
) -> MachineResult<()> {
    let accepted: &[&str] = if possessive {
        &["its", "her", "his"]
    } else {
        &["it", "her", "him"]
    };
    let pronoun = cursor
        .peek()
        .filter(|raw| accepted.iter().any(|word| raw.eq_ignore_ascii_case(word)))
        .map(str::to_string);
    let pronoun = cursor.require_value(pronoun, "the source's pronoun")?;
    cursor.consume(&pronoun)
}

fn recognize_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: ManaCostKeyword,
    cost: &ManaCostNode,
) -> MachineResult<()> {
    cursor.enter("keyword.mana-cost.reminder.body");
    match keyword {
        ManaCostKeyword::Morph | ManaCostKeyword::Megamorph => {
            // CR 702.37c: a face-down spell costs {3}.
            consume_words(
                cursor,
                &[
                    "You", "may", "cast", "this", "card", "face", "down", "as", "a", "2/2",
                    "creature", "for", "{3}", ".", "Turn",
                ],
            )?;
            consume_source_pronoun(cursor, false)?;
            consume_words(cursor, &["face", "up", "any", "time", "for"])?;
            consume_source_pronoun(cursor, true)?;
            consume_words(cursor, &[keyword.word(), "cost"])?;
            if keyword == ManaCostKeyword::Megamorph {
                consume_words(cursor, &["and", "put", "a", "+1/+1", "counter", "on"])?;
                consume_source_pronoun(cursor, false)?;
            }
            cursor.consume(".")
        }
        // CR 702.168a: the face-down spell costs {3} and its permanent has
        // ward {2}.
        ManaCostKeyword::Disguise => {
            consume_words(
                cursor,
                &[
                    "You", "may", "cast", "this", "card", "face", "down", "for", "{3}", "as", "a",
                    "2/2", "creature", "with", "ward", "{2}", ".", "Turn",
                ],
            )?;
            consume_source_pronoun(cursor, false)?;
            consume_words(cursor, &["face", "up", "any", "time", "for"])?;
            consume_source_pronoun(cursor, true)?;
            consume_words(cursor, &["disguise", "cost", "."])
        }
        ManaCostKeyword::Dash => consume_words(
            cursor,
            &[
                "You",
                "may",
                "cast",
                "this",
                "spell",
                "for",
                "its",
                "dash",
                "cost",
                ".",
                "If",
                "you",
                "do",
                ",",
                "it",
                "gains",
                "haste",
                ",",
                "and",
                "it's",
                "returned",
                "from",
                "the",
                "battlefield",
                "to",
                "its",
                "owner's",
                "hand",
                "at",
                "the",
                "beginning",
                "of",
                "the",
                "next",
                "end",
                "step",
                ".",
            ],
        ),
        ManaCostKeyword::Plot => {
            consume_words(cursor, &["You", "may", "pay"])?;
            cursor.enter("keyword.mana-cost.reminder.cost");
            let reminder_cost = recognize_mana_cost(cursor)?;
            cursor.enter("keyword.mana-cost.reminder.cost-match");
            cursor.require(
                cost.same_symbols(&reminder_cost),
                "a reminder cost equal to the printed Plot cost",
            )?;
            cursor.enter("keyword.mana-cost.reminder.body");
            consume_words(
                cursor,
                &[
                    "and", "exile", "this", "card", "from", "your", "hand", ".", "Cast", "it",
                    "as", "a", "sorcery", "on", "a", "later", "turn", "without", "paying", "its",
                    "mana", "cost", ".", "Plot", "only", "as", "a", "sorcery", ".",
                ],
            )
        }
        ManaCostKeyword::Warp => {
            consume_words(
                cursor,
                &[
                    "You", "may", "cast", "this", "card", "from", "your", "hand", "for", "its",
                    "warp", "cost", ".", "Exile", "this",
                ],
            )?;
            let noun = cursor
                .peek()
                .ok_or_else(|| cursor.fail("the source's permanent type"))?;
            cursor.require(
                lookup_permanent_card_type(noun, AcceptedForms::SingularOnly).is_some(),
                "the source's singular permanent type",
            )?;
            cursor.advance_to(cursor.position() + 1)?;
            consume_words(
                cursor,
                &[
                    "at",
                    "the",
                    "beginning",
                    "of",
                    "the",
                    "next",
                    "end",
                    "step",
                    ",",
                    "then",
                    "you",
                    "may",
                    "cast",
                    "it",
                    "from",
                    "exile",
                    "on",
                    "a",
                    "later",
                    "turn",
                    ".",
                ],
            )
        }
    }
}

/// Reads one mana-cost keyword atom and its optional validated reminder.
pub(in crate::oracle::canonical) fn recognize_mana_cost_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaCostKeywordNode> {
    cursor.enter("keyword.mana-cost.keyword");
    let keyword = cursor
        .peek()
        .and_then(ManaCostKeyword::from_word)
        .ok_or_else(|| cursor.fail("a mana-cost keyword"))?;
    cursor.consume(keyword.word())?;
    cursor.enter("keyword.mana-cost.cost");
    let cost = recognize_mana_cost(cursor)?;
    if cursor.peek() == Some("(") {
        cursor.enter("keyword.mana-cost.reminder.open");
        cursor.consume("(")?;
        recognize_reminder(cursor, keyword, &cost)?;
        cursor.enter("keyword.mana-cost.reminder.close");
        cursor.consume(")")?;
    }
    Ok(ManaCostKeywordNode { keyword, cost })
}

/// The engine's keyword IR for the atom (the same shapes its support
/// predicates read): a mana payment and, for megamorph, the counter flag.
pub(in crate::oracle::canonical) fn lower_mana_cost_keyword(
    node: &ManaCostKeywordNode,
) -> serde_json::Value {
    let cost = serde_json::json!({ "kind": "payMana", "manaCost": node.cost.as_oracle_cost() });
    match node.keyword {
        ManaCostKeyword::Morph | ManaCostKeyword::Megamorph => serde_json::json!({
            "kind": "morph",
            "cost": cost,
            "putCounterWhenTurnedFaceUp": node.keyword == ManaCostKeyword::Megamorph,
        }),
        // The face-down casting and turning face up are morph's (CR
        // 702.168b); the runtime gives the face-down permanent ward {2}.
        ManaCostKeyword::Disguise => serde_json::json!({
            "kind": "disguise",
            "cost": cost,
            "putCounterWhenTurnedFaceUp": false,
        }),
        keyword => serde_json::json!({ "kind": keyword.word(), "cost": cost }),
    }
}

#[cfg(test)]
#[path = "mana_cost_keyword_tests.rs"]
mod tests;
