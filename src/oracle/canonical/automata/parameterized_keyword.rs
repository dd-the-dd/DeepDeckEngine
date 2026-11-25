//! Keyword abilities with one printed parameter (CR 702).
//!
//! Each supported keyword is one vocabulary entry: its lexeme, the section of
//! the Comprehensive Rules that defines it, and the kind of parameter it takes.
//! The parameter is read by the shared children (numeric atom, mana/life
//! resource payment, object collection); the keyword never reads its own
//! numbers or costs. An optional reminder commits at `(` and is validated
//! against the printed parameter: either as the keyword's reminder rendering
//! instantiated with that parameter, or, when a shared grammar already reads
//! the reminder sentence, by comparing the typed node it produces with the
//! node the keyword expands to. Lowering into ordinary primitives belongs to
//! the native ability family; this child only produces the typed keyword.
//!
//! ```text
//! Keyword   := LEXEME Parameter [Reminder]
//! Parameter := N                          (count)
//!            | MANA-COST | "—" LIFE-PAYMENT "."   (cost)
//!            | "for" COLLECTION           (quality)
//!            | ε                          (none)
//! Reminder  := "(" ... ")"                (validated, never discarded)
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::recognize_oracle_entities;

/// The parameterized keyword abilities this child reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ParameterizedKeyword {
    /// CR 702.24: cumulative upkeep [cost].
    CumulativeUpkeep,
    /// CR 702.27: buyback [cost].
    Buyback,
    /// CR 702.30: echo [cost].
    Echo,
    /// CR 702.41: affinity for [quality].
    Affinity,
    /// CR 702.74: evoke [cost].
    Evoke,
    /// CR 702.86: annihilator N.
    Annihilator,
    /// CR 702.130: afflict N.
    Afflict,
    /// CR 702.136: riot.
    Riot,
    /// CR 702.164: toxic N.
    Toxic,
    /// CR 702.175: offspring [cost].
    Offspring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParameterKind {
    Count,
    ManaCost,
    ResourceCost,
    Quality,
    None,
}

struct KeywordEntry {
    keyword: ParameterizedKeyword,
    lexeme: &'static [&'static str],
    parameter: ParameterKind,
}

const KEYWORDS: &[KeywordEntry] = &[
    KeywordEntry {
        keyword: ParameterizedKeyword::CumulativeUpkeep,
        lexeme: &["cumulative", "upkeep"],
        parameter: ParameterKind::ResourceCost,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Buyback,
        lexeme: &["buyback"],
        parameter: ParameterKind::ResourceCost,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Echo,
        lexeme: &["echo"],
        parameter: ParameterKind::ResourceCost,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Affinity,
        lexeme: &["affinity"],
        parameter: ParameterKind::Quality,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Afflict,
        lexeme: &["afflict"],
        parameter: ParameterKind::Count,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Evoke,
        lexeme: &["evoke"],
        parameter: ParameterKind::ManaCost,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Annihilator,
        lexeme: &["annihilator"],
        parameter: ParameterKind::Count,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Riot,
        lexeme: &["riot"],
        parameter: ParameterKind::None,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Toxic,
        lexeme: &["toxic"],
        parameter: ParameterKind::Count,
    },
    KeywordEntry {
        keyword: ParameterizedKeyword::Offspring,
        lexeme: &["offspring"],
        parameter: ParameterKind::ManaCost,
    },
];

impl ParameterizedKeyword {
    /// The Comprehensive Rules section that defines the keyword.
    pub(in crate::oracle::canonical) fn rules_section(self) -> &'static str {
        match self {
            Self::CumulativeUpkeep => "702.24",
            Self::Buyback => "702.27",
            Self::Echo => "702.30",
            Self::Affinity => "702.41",
            Self::Evoke => "702.74",
            Self::Annihilator => "702.86",
            Self::Afflict => "702.130",
            Self::Riot => "702.136",
            Self::Toxic => "702.164",
            Self::Offspring => "702.175",
        }
    }

    pub(in crate::oracle::canonical) fn canonical_name(self) -> &'static str {
        match self {
            Self::CumulativeUpkeep => "cumulativeUpkeep",
            Self::Buyback => "buyback",
            Self::Echo => "echo",
            Self::Affinity => "affinity",
            Self::Evoke => "evoke",
            Self::Annihilator => "annihilator",
            Self::Afflict => "afflict",
            Self::Riot => "riot",
            Self::Toxic => "toxic",
            Self::Offspring => "offspring",
        }
    }

    fn entry(self) -> &'static KeywordEntry {
        KEYWORDS
            .iter()
            .find(|entry| entry.keyword == self)
            .expect("every parameterized keyword has one vocabulary entry")
    }
}

/// The printed parameter, already read by its shared child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum KeywordParameterNode {
    None,
    Count(i64),
    Cost(ResourcePaymentNode),
    /// The counted objects named by `for [quality]`, without a controller.
    Quality(ObjectPredicate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ParameterizedKeywordNode {
    pub keyword: ParameterizedKeyword,
    pub parameter: KeywordParameterNode,
    /// True when a printed reminder was validated against the parameter.
    pub reminder: bool,
}

fn lexeme_matches(tokens: &[RecognizedEntity], lexeme: &[&str]) -> bool {
    tokens.len() >= lexeme.len()
        && lexeme
            .iter()
            .zip(tokens)
            .all(|(word, token)| token.raw.eq_ignore_ascii_case(word))
}

fn parameter_starts(kind: ParameterKind, next: Option<&RecognizedEntity>) -> bool {
    let is_numeral = |raw: &str| {
        raw.eq_ignore_ascii_case("x")
            || (!raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()))
    };
    match kind {
        ParameterKind::Count => next.is_some_and(|token| is_numeral(&token.raw)),
        ParameterKind::ManaCost => next.is_some_and(|token| token.token_type == "manaSymbol"),
        ParameterKind::ResourceCost => {
            next.is_some_and(|token| token.token_type == "manaSymbol" || token.raw == "\u{2014}")
        }
        ParameterKind::Quality => next.is_some_and(|token| token.raw.eq_ignore_ascii_case("for")),
        ParameterKind::None => next.is_none_or(|token| token.raw == "("),
    }
}

/// Structural claim: the ability starts with a supported keyword lexeme
/// immediately followed by the start of its parameter kind. A capitalized
/// ability word sharing the lexeme (`Toxic Spores —`) is never claimed.
pub(in crate::oracle::canonical) fn parameterized_keyword_starts(
    tokens: &[RecognizedEntity],
) -> Option<ParameterizedKeyword> {
    KEYWORDS.iter().find_map(|entry| {
        (lexeme_matches(tokens, entry.lexeme)
            && parameter_starts(entry.parameter, tokens.get(entry.lexeme.len())))
        .then_some(entry.keyword)
    })
}

/// The lexeme length of a count keyword (`toxic N`, `annihilator N`,
/// `afflict N`) at the start of `tokens`, followed by its numeral. Only a
/// count keyword is an ability an effect can grant to another object
/// (CR 702.1); a cost or quality parameter belongs to the card itself.
pub(in crate::oracle::canonical) fn grantable_count_keyword_length(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    KEYWORDS.iter().find_map(|entry| {
        (entry.parameter == ParameterKind::Count
            && lexeme_matches(tokens, entry.lexeme)
            && parameter_starts(entry.parameter, tokens.get(entry.lexeme.len())))
        .then_some(entry.lexeme.len())
    })
}

/// The printed parameter and the token range a reminder may quote.
struct ReadParameter {
    node: KeywordParameterNode,
    span: (usize, usize),
    /// Nonmana costs are introduced by an em dash (CR 702.24a, 702.27a).
    dash_cost: bool,
}

fn recognize_parameter(
    cursor: &mut OracleTokenCursor<'_>,
    kind: ParameterKind,
) -> MachineResult<ReadParameter> {
    cursor.enter("keyword.parameterized.parameter");
    let start = cursor.position();
    let mut dash_cost = false;
    let node = match kind {
        ParameterKind::None => KeywordParameterNode::None,
        ParameterKind::Count => match recognize_numeric_atom(cursor)? {
            NumericAtom::Integer(value) if value > 0 => KeywordParameterNode::Count(value),
            _ => return Err(cursor.fail("a positive literal keyword count")),
        },
        ParameterKind::ManaCost => {
            cursor.require(
                cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "manaSymbol"),
                "a mana cost after the keyword",
            )?;
            KeywordParameterNode::Cost(ResourcePaymentNode::Mana(recognize_mana_cost(cursor)?))
        }
        ParameterKind::ResourceCost if cursor.peek() == Some("\u{2014}") => {
            cursor.consume("\u{2014}")?;
            dash_cost = true;
            cursor.enter("keyword.parameterized.cost.nonmana");
            let cost = recognize_resource_payment(cursor)?;
            cursor.require(
                matches!(cost, ResourcePaymentNode::Life(_)),
                "a life payment after the keyword's em dash; other nonmana keyword costs are not supported yet",
            )?;
            KeywordParameterNode::Cost(cost)
        }
        ParameterKind::ResourceCost => {
            KeywordParameterNode::Cost(ResourcePaymentNode::Mana(recognize_mana_cost(cursor)?))
        }
        ParameterKind::Quality => {
            cursor.consume("for")?;
            cursor.enter("keyword.parameterized.quality");
            let collection = recognize_object_collection(cursor)?;
            cursor.require(
                collection.controller.is_none() && !collection.exclude_source,
                "a quality without its own controller relation or source exclusion",
            )?;
            cursor.require(
                collection.predicate.supports_domain(ObjectKind::Permanent)
                    && collection.predicate.has_nominal_head(),
                "a quality naming permanents",
            )?;
            KeywordParameterNode::Quality(collection.predicate)
        }
    };
    let span = (start + usize::from(dash_cost), cursor.position());
    if dash_cost {
        cursor.enter("keyword.parameterized.cost.terminator");
        cursor.consume(".")?;
    }
    if let KeywordParameterNode::Cost(ResourcePaymentNode::Mana(mana)) = &node {
        cursor.enter("keyword.parameterized.cost.mana-support");
        mana.fixed_runtime_mana_value()
            .map_err(|error| cursor.fail(error.expected()))?;
    }
    Ok(ReadParameter {
        node,
        span,
        dash_cost,
    })
}

pub(in crate::oracle::canonical) fn number_word(value: i64) -> Option<&'static str> {
    Some(match value {
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "nine",
        10 => "ten",
        11 => "eleven",
        12 => "twelve",
        13 => "thirteen",
        14 => "fourteen",
        15 => "fifteen",
        16 => "sixteen",
        17 => "seventeen",
        18 => "eighteen",
        19 => "nineteen",
        20 => "twenty",
        _ => return None,
    })
}

/// `a NOUN` for one object, `<number word> NOUNS` otherwise.
fn counted(value: i64, singular: &str, plural: &str) -> Option<String> {
    if value == 1 {
        Some(singular.to_string())
    } else {
        Some(format!("{} {plural}", number_word(value)?))
    }
}

/// Placeholder replaced by the printed cost tokens; never part of Oracle text.
const PRINTED_COST: &str = "PRINTEDCOST";

/// The reminder renderings of the keyword with its printed parameter. Each is
/// a sentence sequence whose `PRINTEDCOST` word stands for the printed cost.
fn reminder_renderings(
    keyword: ParameterizedKeyword,
    parameter: &KeywordParameterNode,
    dash_cost: bool,
) -> Option<Vec<String>> {
    let count = match parameter {
        KeywordParameterNode::Count(value) => Some(*value),
        _ => None,
    };
    Some(match keyword {
        ParameterizedKeyword::Toxic => {
            let poison = counted(count?, "a poison counter", "poison counters")?;
            let base = format!("Players dealt combat damage by this creature also get {poison}.");
            vec![
                format!("{base} A player with ten or more poison counters loses the game."),
                base,
            ]
        }
        ParameterizedKeyword::Evoke => vec![
            "You may cast this spell for its evoke cost. If you do, it's sacrificed when it enters."
                .to_string(),
        ],
        ParameterizedKeyword::Annihilator => vec![format!(
            "Whenever this creature attacks, defending player sacrifices {} of their choice.",
            counted(count?, "a permanent", "permanents")?
        )],
        ParameterizedKeyword::Afflict => vec![format!(
            "Whenever this creature becomes blocked, defending player loses {} life.",
            count?
        )],
        ParameterizedKeyword::Riot => vec![
            "This creature enters with your choice of a +1/+1 counter or haste.".to_string(),
            "This creature enters with your choice of an additional +1/+1 counter or haste."
                .to_string(),
        ],
        ParameterizedKeyword::Offspring => vec![format!(
            "You may pay an additional {PRINTED_COST} as you cast this spell. If you do, when this creature enters, create a 1/1 token copy of it."
        )],
        ParameterizedKeyword::Buyback if dash_cost => vec![format!(
            "You may {PRINTED_COST} in addition to any other costs as you cast this spell. If you do, put this card into your hand as it resolves."
        )],
        ParameterizedKeyword::Buyback => vec![format!(
            "You may pay an additional {PRINTED_COST} as you cast this spell. If you do, put this card into your hand as it resolves."
        )],
        ParameterizedKeyword::CumulativeUpkeep => vec![
            "At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it."
                .to_string(),
        ],
        ParameterizedKeyword::Echo => vec![
            "At the beginning of your upkeep, if this came under your control since the beginning of your last upkeep, sacrifice it unless you pay its echo cost."
                .to_string(),
        ],
        // The reduction sentence is read by the shared casting-reduction child.
        ParameterizedKeyword::Affinity => return None,
    })
}

fn rendering_tokens(rendering: &str, printed_cost: &[RecognizedEntity]) -> Vec<String> {
    recognize_oracle_entities(rendering)
        .into_iter()
        .flat_map(|token| {
            if token.raw == PRINTED_COST {
                printed_cost
                    .iter()
                    .map(|cost| cost.raw.clone())
                    .collect::<Vec<_>>()
            } else {
                vec![token.raw]
            }
        })
        .collect()
}

/// The keyword expansion of `Affinity for [quality]` (CR 702.41a): this spell
/// costs {1} less to cast for each [quality] you control.
pub(in crate::oracle::canonical) fn affinity_reduction(
    quality: &ObjectPredicate,
) -> CastingCostReductionNode {
    CastingCostReductionNode {
        subject: CastingReductionSubject::CurrentSpell,
        amount: NumericExpression::PermanentCount(Box::new(PermanentCountNode {
            predicate: quality.clone(),
            controller: Some(ObjectControllerScope::AbilityController),
            exclude_source: false,
            measure: PermanentCountMeasure::Objects,
        })),
        condition: None,
        target_query: None,
        colored: None,
    }
}

fn recognize_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: ParameterizedKeyword,
    parameter: &ReadParameter,
) -> MachineResult<()> {
    cursor.enter("keyword.parameterized.reminder.open");
    cursor.consume("(")?;
    let body_start = cursor.position();
    let close = cursor.token_slice(body_start, usize::MAX).len() + body_start;
    cursor.require(
        close > body_start
            && cursor
                .token_slice(close - 1, close)
                .first()
                .is_some_and(|token| token.raw == ")"),
        "a reminder closing the keyword ability",
    )?;
    let body_end = close - 1;
    if let (ParameterizedKeyword::Affinity, KeywordParameterNode::Quality(quality)) =
        (keyword, &parameter.node)
    {
        cursor.enter("keyword.parameterized.reminder.reduction");
        let reminder = cursor.bounded(body_end, recognize_casting_cost_reduction)?;
        cursor.enter("keyword.parameterized.reminder.reduction-match");
        cursor.require(
            reminder == affinity_reduction(quality),
            "a reminder reducing this spell by {1} for each permanent of the printed quality you control",
        )?;
    } else {
        cursor.enter("keyword.parameterized.reminder.body");
        let renderings = cursor.require_value(
            reminder_renderings(keyword, &parameter.node, parameter.dash_cost),
            "a reminder rendering for the printed parameter",
        )?;
        let printed_cost = cursor.token_slice(parameter.span.0, parameter.span.1);
        let printed = cursor.token_slice(body_start, body_end);
        let matches = renderings.iter().any(|rendering| {
            let expected = rendering_tokens(rendering, printed_cost);
            expected.len() == printed.len()
                && expected
                    .iter()
                    .zip(printed)
                    .all(|(expected, token)| token.raw.eq_ignore_ascii_case(expected))
        });
        cursor.require(
            matches,
            "the keyword's reminder instantiated with the printed parameter",
        )?;
        cursor.advance_to(body_end)?;
    }
    cursor.enter("keyword.parameterized.reminder.close");
    cursor.consume(")")?;
    Ok(())
}

/// Reads one complete parameterized keyword ability, including its optional
/// reminder. Failures after the lexeme are committed and explicit.
pub(in crate::oracle::canonical) fn recognize_parameterized_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ParameterizedKeywordNode> {
    cursor.enter("keyword.parameterized.lexeme");
    let remaining = cursor.token_slice(cursor.position(), usize::MAX);
    let keyword = cursor.require_value(
        parameterized_keyword_starts(remaining),
        "a supported parameterized keyword followed by its parameter",
    )?;
    let entry = keyword.entry();
    cursor.consume_phrase(entry.lexeme)?;
    let parameter = recognize_parameter(cursor, entry.parameter)?;
    cursor.enter("keyword.parameterized.reminder-or-complete");
    let reminder = cursor.peek() == Some("(");
    if reminder {
        recognize_reminder(cursor, keyword, &parameter)?;
    }
    cursor.enter("keyword.parameterized.complete");
    cursor.finish()?;
    Ok(ParameterizedKeywordNode {
        keyword,
        parameter: parameter.node,
        reminder,
    })
}

#[cfg(test)]
#[path = "parameterized_keyword_tests.rs"]
mod tests;
