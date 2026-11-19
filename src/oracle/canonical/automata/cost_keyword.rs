//! Parameterized casting keywords that are stated as `KEYWORD PARAMETER` with
//! an optional reminder: Plot (CR 702.170a), Foretell (CR 702.143a), Dredge
//! (CR 702.52a) and Splice (CR 702.47a). One automaton owns the shared frame:
//! keyword words, a typed parameter child (fixed mana cost, positive count, or
//! spell quality plus mana cost), and a reminder validated against the printed
//! parameter. The reminder is informative; the authoritative definitions are
//! the cached rules cited by each [`CostKeyword`] and its typed lowering.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{ManaCostNode, ObjectPredicate, recognize_mana_cost, recognize_object_predicate_atom};
use crate::oracle::characteristic_vocabulary::AcceptedForms;

/// A registered parameterized casting keyword.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostKeyword {
    /// CR 702.170a: special action from hand; later free sorcery-speed cast.
    Plot,
    /// CR 702.143a: pay {2} to exile face down; later cast for the foretell cost.
    Foretell,
    /// CR 702.52a: replace a draw with milling N and returning this card.
    Dredge,
    /// CR 702.47a: reveal and pay to add this card's effects to a spell.
    Splice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostKeywordParameter {
    Mana(ManaCostNode),
    Count(i64),
    SpliceOnto {
        quality: ObjectPredicate,
        cost: ManaCostNode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CostKeywordNode {
    pub keyword: CostKeyword,
    pub parameter: CostKeywordParameter,
}

/// One element of a printed reminder. Parameter pieces must restate the
/// printed parameter exactly; they never supply the semantics.
#[derive(Clone, Copy)]
enum ReminderPiece {
    Word(&'static str),
    Words(&'static [&'static str]),
    PrintedMana,
    /// `a card`/`three cards`: the printed count with number agreement.
    PrintedCountCards,
    /// `a`/`an` followed by the printed spell quality and `spell`.
    PrintedSpellQuality,
}

impl CostKeyword {
    pub(in crate::oracle::canonical) fn from_head(raw: &str) -> Option<Self> {
        Some(match raw.to_ascii_lowercase().as_str() {
            "plot" => Self::Plot,
            "foretell" => Self::Foretell,
            "dredge" => Self::Dredge,
            "splice" => Self::Splice,
            _ => return None,
        })
    }

    fn state(self, phase: &'static str) -> &'static str {
        match (self, phase) {
            (Self::Plot, "parameter") => "keyword.plot.parameter",
            (Self::Plot, "reminder") => "keyword.plot.reminder",
            (Self::Foretell, "parameter") => "keyword.foretell.parameter",
            (Self::Foretell, "reminder") => "keyword.foretell.reminder",
            (Self::Dredge, "parameter") => "keyword.dredge.parameter",
            (Self::Dredge, "reminder") => "keyword.dredge.reminder",
            (Self::Splice, "parameter") => "keyword.splice.parameter",
            (Self::Splice, "reminder") => "keyword.splice.reminder",
            (_, _) => "keyword.cost-keyword",
        }
    }

    fn reminder(self) -> &'static [ReminderPiece] {
        use ReminderPiece::{PrintedCountCards, PrintedMana, PrintedSpellQuality, Word, Words};
        match self {
            Self::Plot => &[
                Words(&["You", "may", "pay"]),
                PrintedMana,
                Words(&[
                    "and", "exile", "this", "card", "from", "your", "hand", ".", "Cast", "it",
                    "as", "a", "sorcery", "on", "a", "later", "turn", "without", "paying", "its",
                    "mana", "cost", ".", "Plot", "only", "as", "a", "sorcery", ".",
                ]),
            ],
            // CR 702.143a fixes the exile payment at {2}; it is not the printed cost.
            Self::Foretell => &[Words(&[
                "During", "your", "turn", ",", "you", "may", "pay", "{2}", "and", "exile", "this",
                "card", "from", "your", "hand", "face", "down", ".", "Cast", "it", "on", "a",
                "later", "turn", "for", "its", "foretell", "cost", ".",
            ])],
            Self::Dredge => &[
                Words(&[
                    "If", "you", "would", "draw", "a", "card", ",", "you", "may", "mill",
                ]),
                PrintedCountCards,
                Words(&[
                    "instead",
                    ".",
                    "If",
                    "you",
                    "do",
                    ",",
                    "return",
                    "this",
                    "card",
                    "from",
                    "your",
                    "graveyard",
                    "to",
                    "your",
                    "hand",
                    ".",
                ]),
            ],
            Self::Splice => &[
                Words(&["As", "you", "cast"]),
                PrintedSpellQuality,
                Word(","),
                Words(&[
                    "you", "may", "reveal", "this", "card", "from", "your", "hand", "and", "pay",
                    "its", "splice", "cost", ".", "If", "you", "do", ",", "add", "this", "card's",
                    "effects", "to", "that", "spell", ".",
                ]),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CostKeywordState {
    Keyword,
    Parameter,
    Envelope,
    Reminder,
    ReminderClose,
    Complete,
}

/// Recognizes one complete parameterized casting-keyword ability.
pub(in crate::oracle::canonical) fn recognize_cost_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CostKeywordNode> {
    let mut state = CostKeywordState::Keyword;
    let mut keyword = None;
    let mut parameter = None;
    loop {
        state = match state {
            CostKeywordState::Keyword => {
                cursor.enter("keyword.cost-keyword.keyword");
                let head = cursor
                    .peek()
                    .and_then(CostKeyword::from_head)
                    .ok_or_else(|| cursor.fail("a registered parameterized casting keyword"))?;
                cursor.advance_to(cursor.position() + 1)?;
                keyword = Some(head);
                CostKeywordState::Parameter
            }
            CostKeywordState::Parameter => {
                let keyword = cursor.require_value(keyword, "a recognized keyword")?;
                cursor.enter(keyword.state("parameter"));
                parameter = Some(recognize_parameter(cursor, keyword)?);
                CostKeywordState::Envelope
            }
            CostKeywordState::Envelope => {
                cursor.enter("keyword.cost-keyword.envelope");
                match cursor.peek() {
                    None => CostKeywordState::Complete,
                    Some("(") => {
                        cursor.consume("(")?;
                        CostKeywordState::Reminder
                    }
                    _ => {
                        return Err(
                            cursor.fail("the registered keyword reminder or end of the ability")
                        );
                    }
                }
            }
            CostKeywordState::Reminder => {
                let keyword = cursor.require_value(keyword, "a recognized keyword")?;
                let printed = cursor.require_value(parameter.as_ref(), "a printed parameter")?;
                cursor.enter(keyword.state("reminder"));
                for piece in keyword.reminder() {
                    recognize_reminder_piece(cursor, *piece, printed, keyword.state("reminder"))?;
                }
                CostKeywordState::ReminderClose
            }
            CostKeywordState::ReminderClose => {
                cursor.enter("keyword.cost-keyword.reminder.close");
                cursor.consume(")")?;
                CostKeywordState::Complete
            }
            CostKeywordState::Complete => {
                cursor.enter("keyword.cost-keyword.complete");
                cursor.finish()?;
                return Ok(CostKeywordNode {
                    keyword: cursor.require_value(keyword, "a recognized keyword")?,
                    parameter: cursor.require_value(parameter, "a printed parameter")?,
                });
            }
        };
    }
}

fn recognize_fixed_mana(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
) -> MachineResult<ManaCostNode> {
    cursor.require(
        cursor
            .peek_token()
            .is_some_and(|token| token.token_type == "manaSymbol"),
        "a mana cost; nonmana keyword costs require their own runtime proof",
    )?;
    let cost = recognize_mana_cost(cursor)?;
    cursor.enter(state);
    cost.fixed_runtime_mana_value()
        .map_err(|error| cursor.fail(error.expected()))?;
    Ok(cost)
}

fn recognize_parameter(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: CostKeyword,
) -> MachineResult<CostKeywordParameter> {
    match keyword {
        CostKeyword::Plot | CostKeyword::Foretell => {
            recognize_fixed_mana(cursor, keyword.state("parameter")).map(CostKeywordParameter::Mana)
        }
        CostKeyword::Dredge => {
            let count = cursor
                .peek()
                .filter(|raw| !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()))
                .and_then(|raw| raw.parse::<i64>().ok())
                .filter(|count| *count > 0);
            let count = cursor.require_value(count, "a fixed positive Dredge count")?;
            cursor.advance_to(cursor.position() + 1)?;
            Ok(CostKeywordParameter::Count(count))
        }
        CostKeyword::Splice => {
            cursor.consume("onto")?;
            cursor.enter("keyword.splice.quality");
            let quality = recognize_spell_quality(cursor)?;
            cursor.enter("keyword.splice.cost");
            let cost = recognize_fixed_mana(cursor, "keyword.splice.cost")?;
            Ok(CostKeywordParameter::SpliceOnto { quality, cost })
        }
    }
}

/// `Arcane` or `instant or sorcery`: typed characteristic atoms joined by `or`.
/// The noun `spell` is implicit in the printed keyword and explicit in its
/// reminder; both are read through the same shared atom vocabulary.
fn recognize_spell_quality(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    let mut operands = vec![recognize_object_predicate_atom(
        cursor,
        AcceptedForms::SingularOnly,
    )?];
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        cursor.consume("or")?;
        operands.push(recognize_object_predicate_atom(
            cursor,
            AcceptedForms::SingularOnly,
        )?);
    }
    Ok(if operands.len() == 1 {
        operands.remove(0)
    } else {
        ObjectPredicate::Union(operands)
    })
}

fn recognize_reminder_piece(
    cursor: &mut OracleTokenCursor<'_>,
    piece: ReminderPiece,
    printed: &CostKeywordParameter,
    state: &'static str,
) -> MachineResult<()> {
    match piece {
        ReminderPiece::Word(word) => cursor.consume(word),
        ReminderPiece::Words(words) => {
            for word in words {
                cursor.consume(word)?;
            }
            Ok(())
        }
        ReminderPiece::PrintedMana => {
            let CostKeywordParameter::Mana(printed) = printed else {
                return Err(cursor.fail("a printed mana parameter"));
            };
            let restated = recognize_mana_cost(cursor)?;
            cursor.enter(state);
            cursor.require(
                restated.same_symbols(printed),
                "a reminder cost equal to the printed keyword cost",
            )
        }
        ReminderPiece::PrintedCountCards => {
            let CostKeywordParameter::Count(printed) = printed else {
                return Err(cursor.fail("a printed count parameter"));
            };
            let restated = cursor.peek().and_then(parse_number_word);
            cursor.require(
                restated == Some(*printed),
                "a reminder count equal to the printed keyword count",
            )?;
            cursor.advance_to(cursor.position() + 1)?;
            cursor.consume(if *printed == 1 { "card" } else { "cards" })
        }
        ReminderPiece::PrintedSpellQuality => {
            let CostKeywordParameter::SpliceOnto { quality, .. } = printed else {
                return Err(cursor.fail("a printed spell quality"));
            };
            cursor.require(
                cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                }),
                "an article before the reminder spell quality",
            )?;
            cursor.advance_to(cursor.position() + 1)?;
            let restated = recognize_spell_quality(cursor)?;
            cursor.enter(state);
            cursor.require(
                &restated == quality,
                "a reminder spell quality equal to the printed Splice quality",
            )?;
            cursor.consume("spell")
        }
    }
}

#[cfg(test)]
#[path = "cost_keyword_tests.rs"]
mod tests;
