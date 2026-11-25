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
use crate::oracle::canonical::machine::intern_state;
use crate::oracle::model::RecognizedEntity;

/// Rules verbs written as imperatives in Oracle text that are neither CR 701
/// keyword actions, nor in the tokenizer's action vocabulary, nor in the shared
/// verb lexicon (`verb_lexicon.rs`). Diagnostic data only: it gives these verbs
/// no grammar. A verb that gains a performed lexicon entry (`ALL_LEMMAS`, read
/// below) leaves this list: `flip`, `roll`, `move`, `skip` and `take` did.
const UNSUPPORTED_RULES_VERBS: &[&str] = &[
    "ask",
    "assign",
    "conjure",
    "count",
    "distribute",
    "draft",
    "end",
    "guess",
    "heist",
    "lock",
    "note",
    "repeat",
    "say",
    "seek",
    "separate",
    "switch",
    "turn",
    "unattach",
    "unlock",
    "vote",
];

/// True when `word` (any case) is a rules verb above or its third-person
/// singular form (`rolls`, `separates`, `switches`). Diagnostic only.
pub(in crate::oracle::canonical) fn is_unsupported_rules_verb(word: &str) -> bool {
    let word = word.to_lowercase();
    UNSUPPORTED_RULES_VERBS.contains(&word.as_str())
        || word
            .strip_suffix('s')
            .is_some_and(|base| UNSUPPORTED_RULES_VERBS.contains(&base))
        || word
            .strip_suffix("es")
            .is_some_and(|base| UNSUPPORTED_RULES_VERBS.contains(&base))
}

/// A word reduced to a bounded diagnostic key: lowercase ASCII letters,
/// apostrophes and hyphens, at most 24 characters; anything else is `unkeyed`.
/// Keys built from Oracle vocabulary keep composed diagnostic states bounded.
pub(in crate::oracle::canonical) fn diagnostic_word_key(raw: &str) -> String {
    let key = raw
        .chars()
        .map(|character| match character {
            '\u{2019}' => '\'',
            other => other.to_ascii_lowercase(),
        })
        .collect::<String>();
    let valid = !key.is_empty()
        && key.len() <= 24
        && key
            .chars()
            .all(|character| character.is_ascii_lowercase() || matches!(character, '\'' | '-'));
    if valid { key } else { "unkeyed".to_string() }
}

/// What an unlinked word at a reference position actually is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UnsupportedWord {
    /// A CR 702 keyword ability named by the catalog, without grammar here.
    /// `name` is the catalog's canonical id.
    Keyword {
        end_token: usize,
        name: &'static str,
    },
    /// A keyword action (CR 701) or another rules verb, without grammar here.
    /// `name` is the catalog's canonical id or the lowercase verb.
    Verb {
        end_token: usize,
        name: &'static str,
    },
}

impl UnsupportedWord {
    /// The owner state names the missing keyword or verb itself, so each one
    /// ranks as its own capability (`keyword.unsupported.madness`,
    /// `effect.unsupported-verb.roll`) instead of one shared bucket.
    pub(in crate::oracle::canonical) fn state(&self) -> &'static str {
        match self {
            Self::Keyword { name, .. } => {
                intern_state(format!("keyword.unsupported.{}", diagnostic_word_key(name)))
            }
            Self::Verb { name, .. } => intern_state(format!(
                "effect.unsupported-verb.{}",
                diagnostic_word_key(name)
            )),
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
            Self::Keyword { end_token, .. } | Self::Verb { end_token, .. } => *end_token,
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
                name: mechanic.canonical_id,
            },
            _ => UnsupportedWord::Keyword {
                end_token: mechanic.end_token,
                name: mechanic.canonical_id,
            },
        });
    }
    let word = token.raw.to_lowercase();
    (token.token_type == "actionWord"
        || lookup_verb(&word, &ALL_LEMMAS).is_some()
        || is_unsupported_rules_verb(&word))
    .then(|| UnsupportedWord::Verb {
        end_token: start + 1,
        name: intern_state(diagnostic_word_key(&word)),
    })
}
