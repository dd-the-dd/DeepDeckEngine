//! Diagnostic classification of a word that the name resolver could not link.
//!
//! The name resolver flags capitalized or otherwise unknown words that may be
//! card names. When a reader then expects a source reference at that word,
//! the word is often not a name at all but a mechanic or a verb that has no
//! grammar yet (`Madness`, `Remove`, `Roll`). Naming the real missing concept
//! keeps failure rankings honest. This never changes what is supported: every
//! classification still produces a failure.
use crate::oracle::ability_catalog::{AbilityCategory, match_catalog_mechanic};
use crate::oracle::canonical::automata::verb_lexicon::{ALL_LEMMAS, lookup_verb};
use crate::oracle::model::RecognizedEntity;

/// Rules verbs written as imperatives in Oracle text that are neither CR 701
/// keyword actions, nor in the tokenizer's action vocabulary, nor in the shared
/// verb lexicon (`verb_lexicon.rs`). Diagnostic data only: it gives these verbs
/// no grammar. A verb that gains a lexicon entry leaves this list.
const UNSUPPORTED_RULES_VERBS: &[&str] = &[
    "ask",
    "conjure",
    "count",
    "distribute",
    "draft",
    "flip",
    "heist",
    "lock",
    "move",
    "note",
    "repeat",
    "roll",
    "say",
    "seek",
    "separate",
    "switch",
    "take",
    "unattach",
    "unlock",
];

/// What an unlinked word at a reference position actually is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UnsupportedWord {
    /// A CR 702 keyword ability named by the catalog, without grammar here.
    Keyword { end_token: usize },
    /// A keyword action (CR 701) or another rules verb, without grammar here.
    Verb { end_token: usize },
}

impl UnsupportedWord {
    pub(in crate::oracle::canonical) fn state(&self) -> &'static str {
        match self {
            Self::Keyword { .. } => "keyword.unsupported",
            Self::Verb { .. } => "effect.unsupported-verb",
        }
    }

    pub(in crate::oracle::canonical) fn expected(&self) -> &'static str {
        match self {
            Self::Keyword { .. } => {
                "a keyword ability with a typed, executable rules expansion (CR 702)"
            }
            Self::Verb { .. } => "an action verb supported by the shared effect grammar",
        }
    }

    pub(in crate::oracle::canonical) fn end_token(&self) -> usize {
        match self {
            Self::Keyword { end_token } | Self::Verb { end_token } => *end_token,
        }
    }
}

/// Classifies the unlinked word at `start`: catalog mechanics first, then the
/// tokenizer's action vocabulary and the shared verb lexicon (any inflection),
/// then rules verbs without grammar.
/// `None` keeps the name resolver's diagnostic.
pub(in crate::oracle::canonical) fn classify_unsupported_word(
    tokens: &[RecognizedEntity],
    start: usize,
) -> Option<UnsupportedWord> {
    let token = tokens.get(start)?;
    if let Some(mechanic) = match_catalog_mechanic(tokens, start) {
        return Some(match mechanic.category {
            AbilityCategory::KeywordAction => UnsupportedWord::Verb {
                end_token: mechanic.end_token,
            },
            _ => UnsupportedWord::Keyword {
                end_token: mechanic.end_token,
            },
        });
    }
    let word = token.raw.to_lowercase();
    (token.token_type == "actionWord"
        || lookup_verb(&word, &ALL_LEMMAS).is_some()
        || UNSUPPORTED_RULES_VERBS.contains(&word.as_str()))
    .then(|| UnsupportedWord::Verb {
        end_token: start + 1,
    })
}
