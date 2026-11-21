//! Verb lexicon (data): one entry per lemma with every surface form, the
//! typed argument frame the lemma takes and the voices it admits.
//!
//! The lexicon supplies forms and categories; the clause grammar
//! (`clause.rs`) decides their role. A parent never tests a verb word: it
//! asks the clause reader for a lemma in the frame it expects. Agreement and
//! inflection have one mechanism (`VerbAgreement` -> `VerbForm` ->
//! `VerbEntry::surface`).
//!
//! Lemmas migrate one at a time (`docs/oracle-clause-layer.md`). A verb that
//! has no entry here keeps its established reader until its lemma migrates.

/// A verb lemma with an entry in this lexicon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbLemma {
    /// `gain` (CR 119.3, 119.10 a player gains life).
    Gain,
    /// `lose` (CR 119.3 a player loses life).
    Lose,
}

/// One inflected form of a lemma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbForm {
    /// `gain`: imperative, first/second person, plural, bare infinitive.
    Base,
    /// `gains`.
    ThirdPersonSingular,
    /// `gained` (simple past, `you gained life this turn`).
    Past,
    /// `gained` (perfect and passive).
    PastParticiple,
    /// `gaining`.
    PresentParticiple,
}

/// Agreement of a finite present verb with its grammatical subject. This is
/// the only conjugation choice a subject makes: the subject decides the
/// agreement, the lexicon decides the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbAgreement {
    /// Imperative, `you`, plural subjects, `each` after coordinated subjects,
    /// the bare infinitive after a causative `have` or a modal.
    Base,
    /// A singular third-person subject (`target player`, `each opponent`).
    ThirdPersonSingular,
}

impl VerbAgreement {
    pub(in crate::oracle::canonical) fn form(self) -> VerbForm {
        match self {
            Self::Base => VerbForm::Base,
            Self::ThirdPersonSingular => VerbForm::ThirdPersonSingular,
        }
    }
}

/// The typed argument frame a lemma takes after its verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum ArgumentFrame {
    /// `AMOUNT life` | `life equal to AMOUNT` | bare `life` in patterns: a
    /// change of the acting player's life total (CR 119.3). The slot is the
    /// amount; the resource noun `life` marks the frame.
    LifeTotalChange,
}

impl ArgumentFrame {
    /// The resource noun that marks the frame, when it has one.
    pub(in crate::oracle::canonical) fn resource_noun(self) -> &'static str {
        match self {
            Self::LifeTotalChange => "life",
        }
    }
}

/// Voices a lemma admits. Passive frames (`is dealt`) arrive with the lemmas
/// that need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum Voice {
    Active,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) struct VerbForms {
    pub base: &'static str,
    pub third_person_singular: &'static str,
    pub past: &'static str,
    pub past_participle: &'static str,
    pub present_participle: &'static str,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) struct VerbEntry {
    pub lemma: VerbLemma,
    pub forms: VerbForms,
    pub frames: &'static [ArgumentFrame],
    pub voices: &'static [Voice],
}

impl VerbEntry {
    pub(in crate::oracle::canonical) fn surface(&self, form: VerbForm) -> &'static str {
        match form {
            VerbForm::Base => self.forms.base,
            VerbForm::ThirdPersonSingular => self.forms.third_person_singular,
            VerbForm::Past => self.forms.past,
            VerbForm::PastParticiple => self.forms.past_participle,
            VerbForm::PresentParticiple => self.forms.present_participle,
        }
    }

    /// Every form whose surface is `raw` (`gained` is both past and participle).
    pub(in crate::oracle::canonical) fn forms_of(&self, raw: &str) -> Vec<VerbForm> {
        const ALL: [VerbForm; 5] = [
            VerbForm::Base,
            VerbForm::ThirdPersonSingular,
            VerbForm::Past,
            VerbForm::PastParticiple,
            VerbForm::PresentParticiple,
        ];
        ALL.into_iter()
            .filter(|form| self.surface(*form).eq_ignore_ascii_case(raw))
            .collect()
    }
}

const VERB_LEXICON: &[VerbEntry] = &[
    VerbEntry {
        lemma: VerbLemma::Gain,
        forms: VerbForms {
            base: "gain",
            third_person_singular: "gains",
            past: "gained",
            past_participle: "gained",
            present_participle: "gaining",
        },
        frames: &[ArgumentFrame::LifeTotalChange],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Lose,
        forms: VerbForms {
            base: "lose",
            third_person_singular: "loses",
            past: "lost",
            past_participle: "lost",
            present_participle: "losing",
        },
        frames: &[ArgumentFrame::LifeTotalChange],
        voices: &[Voice::Active],
    },
];

pub(in crate::oracle::canonical) fn verb_entry(lemma: VerbLemma) -> &'static VerbEntry {
    VERB_LEXICON
        .iter()
        .find(|entry| entry.lemma == lemma)
        .expect("every VerbLemma has one lexicon entry")
}

/// The lemma among `lemmas` that `raw` inflects, with every form it can be.
pub(in crate::oracle::canonical) fn lookup_verb(
    raw: &str,
    lemmas: &[VerbLemma],
) -> Option<(&'static VerbEntry, Vec<VerbForm>)> {
    lemmas.iter().find_map(|lemma| {
        let entry = verb_entry(*lemma);
        let forms = entry.forms_of(raw);
        (!forms.is_empty()).then_some((entry, forms))
    })
}

/// True when `raw` is the given form of the lemma. Lookahead gates of other
/// families use this instead of a literal verb word.
pub(in crate::oracle::canonical) fn is_verb_form(
    raw: &str,
    lemma: VerbLemma,
    form: VerbForm,
) -> bool {
    verb_entry(lemma).surface(form).eq_ignore_ascii_case(raw)
}

#[cfg(test)]
#[path = "verb_lexicon_tests.rs"]
mod tests;
