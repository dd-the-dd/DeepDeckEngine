//! Ordered lexical definitions compiled once into an immutable binary index.
//!
//! Finite words retain the first matching definition, including open-pattern
//! precedence. Tokens outside that finite vocabulary only need open patterns.
use std::sync::OnceLock;

use super::{is_object_type_word, is_ordinal_token, is_power_toughness_value, is_quantity_token};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, ObjectCharacteristic, lookup_amass_army_subtype, lookup_object_characteristic,
};

pub(super) type TokenClass = (&'static str, &'static str);
const UNKNOWN: TokenClass = ("unknown", "unknown");

pub(super) const OBJECT_TYPE_WORDS: &[&str] = &[
    "artifact",
    "artifacts",
    "battle",
    "battles",
    "card",
    "creature",
    "creatures",
    "enchantment",
    "enchantments",
    "instant",
    "instants",
    "land",
    "lands",
    "permanent",
    "permanents",
    "planeswalker",
    "planeswalkers",
    "player",
    "players",
    "sorcery",
    "sorceries",
    "spell",
    "spells",
];

#[derive(Clone, Copy)]
enum OpenPattern {
    ManaSymbol,
    Ordinal,
    ObjectPossessive,
    NegativeObjectType,
    NegativeCharacteristic,
    Commander,
    ArmySubtype,
    CounterValue,
    Quantity,
}

impl OpenPattern {
    fn classify(self, raw: &str, normalized: &str) -> Option<TokenClass> {
        match self {
            Self::ManaSymbol if raw.starts_with('{') && raw.ends_with('}') && raw.len() >= 3 => {
                Some(match normalized {
                    "{e}" => ("magicVocabulary", "counterSymbol"),
                    "{t}" | "{q}" => ("cost", "tapSymbol"),
                    _ => ("cost", "manaSymbol"),
                })
            }
            Self::Ordinal if is_ordinal_token(raw) => Some(("logic", "ordinalWord")),
            Self::ObjectPossessive
                if normalized
                    .strip_suffix("'s")
                    .or_else(|| normalized.strip_suffix("’s"))
                    .is_some_and(is_object_type_word) =>
            {
                Some(("entityReference", "possessiveTypeWord"))
            }
            Self::NegativeObjectType
                if normalized
                    .strip_prefix("non")
                    .is_some_and(is_object_type_word) =>
            {
                Some(("entityReference", "negativeTypeWord"))
            }
            Self::NegativeCharacteristic
                if normalized.strip_prefix("non").is_some_and(|word| {
                    lookup_object_characteristic(word, AcceptedForms::SingularOnly).is_some()
                }) =>
            {
                Some(("entityReference", "negativeCharacteristicWord"))
            }
            Self::Commander
                if lookup_object_characteristic(raw, AcceptedForms::SingularOrPlural)
                    == Some(ObjectCharacteristic::Commander) =>
            {
                Some(("entityReference", "objectCharacteristicWord"))
            }
            Self::ArmySubtype
                if lookup_amass_army_subtype(raw, AcceptedForms::SingularOrPlural).is_some() =>
            {
                Some(("entityReference", "subtypeWord"))
            }
            Self::CounterValue if is_power_toughness_value(raw) => {
                Some(("magicVocabulary", "counterValue"))
            }
            Self::Quantity if is_quantity_token(raw) => Some(("logic", "quantity")),
            _ => None,
        }
    }
}

enum LexicalRule {
    Words(&'static [&'static str], TokenClass),
    Open(OpenPattern),
}

use LexicalRule::{Open, Words};
use OpenPattern::*;

// Declaration order is precedence. Repeated words are resolved during build,
// so adding a definition cannot silently change an earlier classification.
const RULES: &[LexicalRule] = &[
    Open(ManaSymbol),
    Open(Ordinal),
    Words(&["when", "whenever", "at"], ("trigger", "triggerWord")),
    Words(&["combat"], ("trigger", "eventQualifierWord")),
    Words(
        &[
            "die", "dies", "died", "enter", "enters", "entered", "entering", "leave", "leaves",
            "left", "attacks",
        ],
        ("trigger", "eventWord"),
    ),
    Words(&["cause"], ("trigger", "causationWord")),
    Words(&["trigger"], ("trigger", "triggerActionWord")),
    Words(
        &[
            "if", "unless", "and", "or", "not", "then", "instead", "don't", "either",
        ],
        ("logic", "logicWord"),
    ),
    Words(&["except"], ("logic", "exceptionWord")),
    Words(&["number", "up"], ("logic", "cardinalityWord")),
    Words(
        &[
            "half",
            "plus",
            "minus",
            "times",
            "multiplied",
            "divided",
            "twice",
            "thrice",
        ],
        ("logic", "numericOperatorWord"),
    ),
    Words(&["rounded"], ("logic", "roundingWord")),
    Words(&["down"], ("syntax", "directionWord")),
    Words(&["can", "may"], ("logic", "modalityWord")),
    Words(&["many"], ("entityReference", "quantityAnaphorWord")),
    Words(&["are"], ("syntax", "copulaWord")),
    Words(&["would"], ("logic", "replacementModalWord")),
    Words(&["rather", "than"], ("logic", "substitutionWord")),
    Words(&["without"], ("cost", "costOmissionWord")),
    Words(&["the"], ("logic", "determinerWord")),
    Words(&["there"], ("logic", "existentialWord")),
    Words(
        &["a", "an", "all", "another", "any", "each", "every"],
        ("logic", "quantifierWord"),
    ),
    Words(
        &[
            "this", "that", "those", "her", "his", "him", "it", "it's", "its", "you", "your",
            "they", "they're", "them", "their", "that's", "that’s", "other",
        ],
        ("entityReference", "referenceWord"),
    ),
    Words(&["target"], ("entityReference", "targetMarker")),
    Words(&["targets"], ("entityReference", "targetNoun")),
    Words(
        &[
            "commander's",
            "commander’s",
            "owner's",
            "owner’s",
            "opponent's",
            "opponent’s",
            "player's",
            "player’s",
        ],
        ("entityReference", "possessivePlayerRoleWord"),
    ),
    Open(ObjectPossessive),
    Words(&["card's"], ("entityReference", "possessiveTypeWord")),
    Words(
        &["spell's", "spell’s"],
        ("entityReference", "possessiveTypeWord"),
    ),
    Words(&["turn's", "turn’s"], ("syntax", "possessiveTurnWord")),
    Words(
        &["basic", "legendary"],
        ("entityReference", "supertypeWord"),
    ),
    Words(&["nonbasic"], ("entityReference", "negativeSupertypeWord")),
    Open(NegativeObjectType),
    Open(NegativeCharacteristic),
    Words(OBJECT_TYPE_WORDS, ("entityReference", "typeWord")),
    Open(Commander),
    Words(
        &["plains", "island", "swamp", "mountain", "forest"],
        ("entityReference", "subtypeWord"),
    ),
    Open(ArmySubtype),
    Words(
        &[
            "add",
            "activate",
            "amass",
            "attach",
            "attack",
            "block",
            "becomes",
            "cast",
            "casting",
            "casts",
            "choose",
            "chooses",
            "copy",
            "counter",
            "countered",
            "create",
            "creates",
            "deal",
            "deals",
            "destroy",
            "change",
            "discard",
            "discards",
            "draw",
            "draws",
            "empower",
            "exile",
            "exiled",
            "exiles",
            "explore",
            "explores",
            "gain",
            "gains",
            "get",
            "gets",
            "investigate",
            "look",
            "lose",
            "loses",
            "mill",
            "mills",
            "play",
            "plays",
            "produce",
            "proliferate",
            "proliferates",
            "put",
            "putting",
            "puts",
            "return",
            "reveal",
            "reveals",
            "sacrifice",
            "search",
            "searches",
            "shuffle",
            "shuffles",
        ],
        ("magicVocabulary", "actionWord"),
    ),
    Words(&["while"], ("syntax", "temporalConnectorWord")),
    Words(&["could"], ("logic", "modalWord")),
    Words(&["kicked"], ("magicVocabulary", "keywordStateWord")),
    Words(
        &["activated", "triggered"],
        ("magicVocabulary", "abilityKindWord"),
    ),
    Words(
        &[
            "ability",
            "abilities",
            "cards",
            "copies",
            "counters",
            "emblem",
            "effect",
            "source",
            "sources",
            "token",
            "tokens",
        ],
        ("magicVocabulary", "objectWord"),
    ),
    Words(
        &[
            "also",
            "alone",
            "back",
            "matter",
            "new",
            "next",
            "only",
            "random",
            "still",
            "transformed",
            "way",
        ],
        ("magicVocabulary", "qualifierWord"),
    ),
    Words(&["until"], ("syntax", "durationBoundaryWord")),
    Words(&["end"], ("syntax", "durationPointWord")),
    Words(&["top", "bottom"], ("magicVocabulary", "zonePositionWord")),
    Words(&["order"], ("magicVocabulary", "collectionOrderWord")),
    Words(&["rest"], ("magicVocabulary", "collectionRemainderWord")),
    Words(&["time", "turn", "turns"], ("logic", "repetitionUnitWord")),
    Words(&["step", "steps"], ("syntax", "turnStepWord")),
    Words(&["upkeep"], ("syntax", "turnStepWord")),
    Words(&["beginning"], ("syntax", "temporalMarkerWord")),
    Words(
        &["pay", "paid", "paying", "pays", "spend"],
        ("cost", "costActionWord"),
    ),
    Words(
        &["additional", "mana", "cost", "costs"],
        ("cost", "costWord"),
    ),
    Words(
        &["white", "blue", "black", "red", "green", "colorless"],
        ("magicVocabulary", "colorWord"),
    ),
    Words(&["color"], ("magicVocabulary", "colorNoun")),
    Words(&["identity"], ("magicVocabulary", "identityWord")),
    Words(
        &[
            "battlefield",
            "graveyard",
            "graveyards",
            "hand",
            "hands",
            "library",
            "libraries",
        ],
        ("magicVocabulary", "zoneWord"),
    ),
    Words(&["anywhere"], ("magicVocabulary", "zoneScopeWord")),
    Words(&["prepared", "tapped"], ("magicVocabulary", "stateWord")),
    Words(&["attacking"], ("magicVocabulary", "combatStateWord")),
    Words(
        &["opponent", "opponents"],
        ("entityReference", "playerQualifierWord"),
    ),
    Words(&["control", "controls"], ("magicVocabulary", "controlWord")),
    Words(
        &["controller", "owner"],
        ("entityReference", "playerRoleWord"),
    ),
    Words(&["damage", "life"], ("magicVocabulary", "resourceWord")),
    Words(
        &["power", "toughness", "value"],
        ("magicVocabulary", "propertyWord"),
    ),
    Words(
        &["name", "size", "text", "type", "types"],
        ("magicVocabulary", "propertyWord"),
    ),
    Words(&["amount"], ("logic", "quantityNoun")),
    Words(&["energy"], ("magicVocabulary", "counterTypeWord")),
    Words(&["chosen"], ("entityReference", "selectionReferenceWord")),
    Words(
        &["equal", "less", "greater", "more", "same", "least", "most"],
        ("logic", "comparisonWord"),
    ),
    Words(&["can't", "can’t"], ("logic", "logicWord")),
    Words(
        &["be", "do", "has", "have", "is", "was", "were"],
        ("syntax", "predicateWord"),
    ),
    Words(
        &["wasn't", "wasn’t", "doesn't", "doesn’t"],
        ("syntax", "negativePredicateWord"),
    ),
    Words(&["equipped"], ("entityReference", "attachmentStateWord")),
    Words(&["came", "under"], ("syntax", "relationWord")),
    Words(
        &["addition", "maximum", "only"],
        ("magicVocabulary", "qualifierWord"),
    ),
    Words(&["no"], ("logic", "logicWord")),
    Open(CounterValue),
    Words(
        &[
            "as", "at", "by", "for", "from", "among", "in", "into", "of", "on", "onto", "to",
            "where", "with",
        ],
        ("syntax", "relationWord"),
    ),
    Words(
        &[":", ",", ".", ";", "—", "•", "+", "-", "−", "(", ")", "\""],
        ("boundary", "separator"),
    ),
    Open(Quantity),
    // A paired characteristic delimiter is not an arithmetic division token.
    Words(&["/"], ("boundary", "statPairSeparator")),
];

struct WordEntry {
    word: &'static str,
    class: TokenClass,
}

struct TokenVocabulary {
    words: Box<[WordEntry]>,
    open_patterns: Box<[OpenPattern]>,
}

impl TokenVocabulary {
    fn build() -> Self {
        let mut keys: Vec<_> = RULES
            .iter()
            .filter_map(|rule| match rule {
                Words(words, _) => Some(*words),
                Open(_) => None,
            })
            .flatten()
            .copied()
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let words = keys
            .into_iter()
            .map(|word| {
                let class = RULES
                    .iter()
                    .find_map(|rule| match rule {
                        Words(words, class) => words.contains(&word).then_some(*class),
                        Open(pattern) => pattern.classify(word, word),
                    })
                    .expect("each indexed word has a declared lexical definition");
                WordEntry { word, class }
            })
            .collect();
        let open_patterns = RULES
            .iter()
            .filter_map(|rule| match rule {
                Open(pattern) => Some(*pattern),
                Words(_, _) => None,
            })
            .collect();
        Self {
            words,
            open_patterns,
        }
    }

    fn classify(&self, raw: &str) -> TokenClass {
        let normalized = raw.to_ascii_lowercase();
        if let Ok(index) = self
            .words
            .binary_search_by_key(&normalized.as_str(), |entry| entry.word)
        {
            return self.words[index].class;
        }
        self.open_patterns
            .iter()
            .find_map(|pattern| pattern.classify(raw, &normalized))
            .unwrap_or(UNKNOWN)
    }
}

fn vocabulary() -> &'static TokenVocabulary {
    static VOCABULARY: OnceLock<TokenVocabulary> = OnceLock::new();
    VOCABULARY.get_or_init(TokenVocabulary::build)
}

pub(super) fn classify(raw: &str) -> TokenClass {
    vocabulary().classify(raw)
}

#[cfg(test)]
#[path = "token_vocabulary_tests.rs"]
mod tests;
