//! Inline resolution choices written inside a noun phrase (CR 608.2d):
//!
//! ```text
//! InlineChoice := "of" "the" CHARACTERISTIC "of" "your" "choice"
//! CHARACTERISTIC := "creature" "type" | "color"
//! ```
//!
//! `creatures of the creature type of your choice get +2/+2` is the same
//! program as `Choose a creature type. Creatures of the chosen type get
//! +2/+2`: the controller chooses while the effect resolves, then the shared
//! object query reads the chosen characteristic. The phrase is two concepts,
//! never a combined operation: the sequence emits the ordinary `choose`
//! instruction before the clause whose noun phrase writes the choice, and
//! the selector reads the phrase as the ordinary chosen-characteristic
//! predicate (`of the chosen type`).
//!
//! The selector may read the phrase anywhere; the program lowering and the
//! rule binding validation refuse a chosen-characteristic reference without
//! its one producing choice. A declared target (`target creature of the
//! creature type of your choice`) is chosen as the spell is cast (CR
//! 601.2c), before any resolution choice, so no resolution choice is emitted
//! for it and the reference fails explicitly.
use super::super::OracleTokenCursor;
use super::{ChoiceDomainNode, ChooseInstructionNode, ChooserNode};

/// The characteristic an inline choice names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum InlineChoiceKind {
    CreatureType,
    Color,
}

impl InlineChoiceKind {
    const ALL: [Self; 2] = [Self::CreatureType, Self::Color];

    fn characteristic(self) -> &'static [&'static str] {
        match self {
            Self::CreatureType => &["creature", "type"],
            Self::Color => &["color"],
        }
    }

    fn domain(self) -> ChoiceDomainNode {
        match self {
            Self::CreatureType => ChoiceDomainNode::CreatureType,
            Self::Color => ChoiceDomainNode::Color,
        }
    }
}

/// How far a clause head may write its noun phrase before the inline
/// choice: `creatures of`, `all creatures of`, `destroy all creatures of`,
/// `untap all creatures you control of`.
const HEAD_SCAN_LIMIT: usize = 5;

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// The width of `of the CHARACTERISTIC of your choice` at `offset`.
fn inline_choice_width_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    kind: InlineChoiceKind,
) -> Option<usize> {
    if !(word_at(cursor, offset, "of") && word_at(cursor, offset + 1, "the")) {
        return None;
    }
    let mut at = offset + 2;
    for word in kind.characteristic() {
        if !word_at(cursor, at, word) {
            return None;
        }
        at += 1;
    }
    (word_at(cursor, at, "of")
        && word_at(cursor, at + 1, "your")
        && word_at(cursor, at + 2, "choice"))
    .then_some(at + 3 - offset)
}

/// The width of the inline choice of `kind` at the cursor, read by the
/// selector as the chosen-characteristic qualifier.
pub(in crate::oracle::canonical) fn inline_choice_qualifier_length(
    cursor: &OracleTokenCursor<'_>,
    kind: InlineChoiceKind,
) -> Option<usize> {
    inline_choice_width_at(cursor, 0, kind)
}

/// A word that opens a structure other than the clause's own noun phrase
/// (a preposed condition, trigger, duration or iteration) or a declared
/// target, whose characteristic is chosen as the spell is cast.
fn ends_head_scan(raw: &str) -> bool {
    matches!(raw, "." | "," | ";" | ":" | "(" | "\u{201c}" | "\"")
        || [
            "if", "when", "whenever", "as", "until", "for", "unless", "target", "targets",
            "choose", "chooses", "where",
        ]
        .iter()
        .any(|word| raw.eq_ignore_ascii_case(word))
}

/// The resolution choice a clause starting at the cursor writes inline in
/// its head noun phrase, as the ordinary `choose` instruction its program
/// performs first. `None` when no inline choice is written there.
pub(in crate::oracle::canonical) fn clause_inline_choice(
    cursor: &OracleTokenCursor<'_>,
) -> Option<ChooseInstructionNode> {
    for offset in 1..=HEAD_SCAN_LIMIT {
        if cursor.peek_ahead(offset - 1).is_none_or(ends_head_scan) {
            return None;
        }
        if let Some(kind) = InlineChoiceKind::ALL
            .into_iter()
            .find(|kind| inline_choice_width_at(cursor, offset, *kind).is_some())
        {
            return Some(ChooseInstructionNode {
                chooser: ChooserNode::Controller,
                domain: kind.domain(),
            });
        }
    }
    None
}

#[cfg(test)]
#[path = "inline_choice_tests.rs"]
mod tests;
