//! The head of an effect clause (S3): `SUBJECT x LEMMA x MODALITY`, read
//! once before the effect dispatcher chooses a frame.
//!
//! ```text
//! Head      := Preposed? Subject Modality? Verb
//! Preposed  := "for" "each" "color"            (the mana output owns it)
//! Subject   := (none: imperative)
//!            | "you"                           (the controller, CR 109.5)
//!            | "its" "controller" | "that" NOUN "controller"
//!                                              (an object-related player)
//!            | ("its" | "that") NOUN           (a possessed player, before `may`)
//!            | "target" | "another" | "up" | QUANTITY
//!                                              (a declared operand)
//!            | "each" ...                      (a quantified player or object set)
//!            | anything else                   (an object phrase, a source, an anaphor)
//! Modality  := "may" (CR 608.2d)
//! Verb      := any lemma of the verb lexicon, in any form
//! ```
//!
//! The head never consumes: it is a classification of the written tokens
//! that the dispatcher (`sequence.rs::recognize_effect`) matches on. Every
//! verb comes from the lexicon (`verb_lexicon.rs`); a verb word is never
//! tested by the dispatcher itself. The subject kinds are only the heads the
//! frames distinguish; the frames read and bind the subject themselves.
use super::super::OracleTokenCursor;
use super::{
    Auxiliary, ClauseModality, VerbForm, VerbLemma, controller_reference_head_length,
    lookup_auxiliary, lookup_effect_verb,
};

/// Who the head of an effect clause names as its subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum EffectSubjectKind {
    /// No subject: the verb opens the clause and the controller performs it.
    Imperative,
    /// `for each color, add ...`: a preposed iteration the mana output reads.
    ColorIteration,
    /// `you VERB`: the controller named explicitly (CR 109.5).
    Controller,
    /// `its controller` / `that NOUN's controller` (CR 109.4).
    ObjectRelatedPlayer,
    /// `its|that NOUN may VERB`: a possessed player before a modal.
    Possessed,
    /// `target ...`, `another ...`, `up to ...`, a written quantity: a
    /// declared operand (object or player).
    Declared,
    /// `each ...`: a quantified player or object set.
    Quantified,
    /// Any other head: an object phrase, a source reference, an anaphor.
    Other,
}

/// The classified head of an effect clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EffectHead {
    pub subject: EffectSubjectKind,
    pub modality: ClauseModality,
    /// The lexicon lemma at `verb_offset`, when the word there is a verb.
    /// A noun-phrase subject (declared, quantified, other) has none: its
    /// frames read the phrase before they find the verb.
    pub lemma: Option<VerbLemma>,
    /// Every form the written verb can be (`put` is base and past).
    pub forms: Vec<VerbForm>,
    /// Token offset of the verb from the start of the clause.
    pub verb_offset: usize,
}

impl EffectHead {
    /// True when the verb is `lemma` in `form`.
    pub(in crate::oracle::canonical) fn is(&self, lemma: VerbLemma, form: VerbForm) -> bool {
        self.lemma == Some(lemma) && self.forms.contains(&form)
    }

    /// True when the verb is the base form of `lemma` (imperative, `you`,
    /// after a modal).
    pub(in crate::oracle::canonical) fn base(&self, lemma: VerbLemma) -> bool {
        self.is(lemma, VerbForm::Base)
    }

    /// The imperative lemma, when the clause has no subject.
    pub(in crate::oracle::canonical) fn imperative(&self) -> Option<VerbLemma> {
        (self.subject == EffectSubjectKind::Imperative)
            .then_some(self.lemma)
            .flatten()
    }
}

/// The determiner after a verb, as the object frames distinguish it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum OperandDeterminer {
    /// `all` / `each`: a universal collection.
    Universal,
    /// `those`: the bound set of an earlier action.
    Those,
    /// `that`: a bound singular reference.
    That,
    /// `it`: the singular anaphor.
    It,
    /// `target`: a declaration.
    Target,
    /// Any other word.
    Other,
    /// Nothing follows.
    Absent,
}

/// The determiner at `offset` from the cursor.
pub(in crate::oracle::canonical) fn operand_determiner(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> OperandDeterminer {
    const DETERMINERS: &[(&str, OperandDeterminer)] = &[
        ("all", OperandDeterminer::Universal),
        ("each", OperandDeterminer::Universal),
        ("those", OperandDeterminer::Those),
        ("that", OperandDeterminer::That),
        ("it", OperandDeterminer::It),
        ("target", OperandDeterminer::Target),
    ];
    match cursor.peek_ahead(offset) {
        None => OperandDeterminer::Absent,
        Some(raw) => DETERMINERS
            .iter()
            .find(|(word, _)| raw.eq_ignore_ascii_case(word))
            .map_or(OperandDeterminer::Other, |(_, determiner)| *determiner),
    }
}

/// Subject words the head distinguishes before reading a verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadWord {
    You,
    Its,
    That,
    Target,
    Another,
    Up,
    Each,
    For,
    Color,
}

fn head_word(raw: &str) -> Option<HeadWord> {
    const WORDS: &[(&str, HeadWord)] = &[
        ("you", HeadWord::You),
        ("its", HeadWord::Its),
        ("that", HeadWord::That),
        ("target", HeadWord::Target),
        ("another", HeadWord::Another),
        ("up", HeadWord::Up),
        ("each", HeadWord::Each),
        ("for", HeadWord::For),
        ("color", HeadWord::Color),
    ];
    WORDS
        .iter()
        .find(|(word, _)| raw.eq_ignore_ascii_case(word))
        .map(|(_, head)| *head)
}

/// Classifies the head of the effect clause at the cursor without consuming.
pub(in crate::oracle::canonical) fn read_effect_head(cursor: &OracleTokenCursor<'_>) -> EffectHead {
    let word = |offset: usize| cursor.peek_ahead(offset).and_then(head_word);
    let may_at = |offset: usize| {
        cursor
            .peek_ahead(offset)
            .and_then(lookup_auxiliary)
            .is_some_and(|(auxiliary, _)| auxiliary == Auxiliary::May)
    };
    let head = |subject: EffectSubjectKind, modality: ClauseModality, verb_offset: usize| {
        let (lemma, forms) = cursor
            .peek_ahead(verb_offset)
            .and_then(lookup_effect_verb)
            .map_or((None, Vec::new()), |(entry, forms)| {
                (Some(entry.lemma), forms)
            });
        EffectHead {
            subject,
            modality,
            lemma,
            forms,
            verb_offset,
        }
    };
    if word(0) == Some(HeadWord::For)
        && word(1) == Some(HeadWord::Each)
        && word(2) == Some(HeadWord::Color)
    {
        return EffectHead {
            subject: EffectSubjectKind::ColorIteration,
            modality: ClauseModality::Indicative,
            lemma: Some(VerbLemma::Add),
            forms: vec![VerbForm::Base],
            verb_offset: 0,
        };
    }
    let imperative = head(EffectSubjectKind::Imperative, ClauseModality::Indicative, 0);
    if imperative.forms.contains(&VerbForm::Base) {
        return imperative;
    }
    match word(0) {
        Some(HeadWord::You) if may_at(1) => {
            return head(EffectSubjectKind::Controller, ClauseModality::May, 2);
        }
        Some(HeadWord::You) => {
            return head(EffectSubjectKind::Controller, ClauseModality::Indicative, 1);
        }
        Some(HeadWord::Its | HeadWord::That) if may_at(2) => {
            return head(EffectSubjectKind::Possessed, ClauseModality::May, 3);
        }
        _ => {}
    }
    if let Some(length) = controller_reference_head_length(
        cursor.token_slice(cursor.position(), cursor.position() + 3),
    ) {
        return if may_at(length) {
            head(
                EffectSubjectKind::ObjectRelatedPlayer,
                ClauseModality::May,
                length + 1,
            )
        } else {
            head(
                EffectSubjectKind::ObjectRelatedPlayer,
                ClauseModality::Indicative,
                length,
            )
        };
    }
    let declared = matches!(
        word(0),
        Some(HeadWord::Target | HeadWord::Another | HeadWord::Up)
    ) || cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity");
    // A noun-phrase subject: its frames read the phrase and find the verb.
    let subject = if declared {
        EffectSubjectKind::Declared
    } else if word(0) == Some(HeadWord::Each) {
        EffectSubjectKind::Quantified
    } else {
        EffectSubjectKind::Other
    };
    EffectHead {
        subject,
        modality: ClauseModality::Indicative,
        lemma: None,
        forms: Vec::new(),
        verb_offset: 0,
    }
}

#[cfg(test)]
#[path = "effect_head_tests.rs"]
mod tests;
