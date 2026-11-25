//! Casting keywords (CR 702): keywords that state a rule about casting their
//! own spell — an alternative cost (CR 118.9, 601.2b), an optional additional
//! cost (CR 601.2f), or a trigger of the cast itself. Every row shares one
//! line grammar:
//!
//! ```text
//! Line      := LEXEME Parameter ["."] [ "(" Reminder ")" ]
//! Parameter := ε                         (conspire, gravestorm, paradigm,
//!                                          retrace, jump-start, aftermath)
//!            | N                         (teamwork)
//!            | MANA                      (spectacle, surge, web-slinging, mayhem,
//!                                          emerge, sneak, bestow, disturb)
//!            | MANA | "—" COSTS          (blitz, escalate, entwine, madness)
//!            | "—" COSTS                 (escape, nonmana kicker)
//!            | N "—" MANA                (impending, awaken)
//!            | MANA "—" P "/" T          (prototype)
//! COSTS     := the shared activation-cost sequence (`compound_activated.rs`)
//! Reminder  := one of the row's registered renderings, instantiated with the
//!              printed parameter (`$cost`, `$N`, `$Nw`), compared token by
//!              token
//!            | "You may" COSTS "in addition to any other costs as you cast
//!              this spell." — an additional-cost reminder restating the
//!              printed payments, read again by the shared cost sequence
//! ```
//!
//! This child only types the line: the lexeme, the parameter read by the
//! shared numeric, mana-cost and cost-sequence children, and the validated
//! reminder. Each keyword's rules definition is lowered by
//! `abilities/alternative_cost_native.rs`: the alternative-cost rows through
//! the one alternative-cast permission lowering, the others to the runtime
//! that executes that keyword. A reminder is never skipped as text, and a
//! parameter shape a row does not register fails explicitly.
use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::{
    ActivationCostSequenceNode, KeywordTemplateArguments, LocalActivationCostNode, ManaCostNode,
    NumericAtom, instantiate, recognize_activation_cost_sequence, recognize_mana_cost,
    recognize_numeric_atom,
};
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::recognize_entities;

/// A casting keyword row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastingKeyword {
    /// CR 702.127a: the half of a split card castable only from a graveyard,
    /// exiled whenever it leaves the stack after that cast.
    Aftermath,
    Awaken,
    /// CR 702.103a.
    Bestow,
    Blitz,
    Conspire,
    /// CR 702.146a: cast the card transformed from its owner's graveyard
    /// for this cost rather than its mana cost.
    Disturb,
    /// CR 702.119a.
    Emerge,
    /// CR 702.42a.
    Entwine,
    Escalate,
    Escape,
    /// CR 702.174a with the gift `a card` (the recipient draws a card).
    GiftACard,
    Gravestorm,
    Impending,
    /// CR 702.133a.
    JumpStart,
    /// CR 702.33a with a nonmana cost (`Kicker—Sacrifice a land.`); a mana
    /// kicker cost is read by `keyword.rs`.
    Kicker,
    /// CR 702.35a: a discard replaced by an exile and the linked trigger
    /// that casts the card for its madness cost.
    Madness,
    Mayhem,
    Paradigm,
    Prototype,
    /// CR 702.81a.
    Retrace,
    /// CR 702.190a.
    Sneak,
    Spectacle,
    Surge,
    Teamwork,
    WebSlinging,
}

impl CastingKeyword {
    /// The CR clause defining the keyword.
    pub(in crate::oracle::canonical) fn rules_section(self) -> &'static str {
        definition(self).rules_section
    }

    pub(in crate::oracle::canonical) fn lexeme(self) -> &'static str {
        definition(self).lexeme
    }
}

/// The printed parameter shape of a casting keyword row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parameter {
    None,
    Count,
    Mana,
    ManaOrCostSequence,
    CostSequence,
    CountThenMana,
    ManaThenCharacteristics,
}

struct Definition {
    keyword: CastingKeyword,
    lexeme: &'static str,
    rules_section: &'static str,
    parameter: Parameter,
    /// Accepted reminder renderings, without parentheses.
    reminders: &'static [&'static str],
}

const DEFINITIONS: &[Definition] = &[
    Definition {
        keyword: CastingKeyword::Aftermath,
        lexeme: "aftermath",
        rules_section: "702.127a",
        parameter: Parameter::None,
        reminders: &["Cast this spell only from your graveyard. Then exile it."],
    },
    Definition {
        keyword: CastingKeyword::Awaken,
        lexeme: "awaken",
        rules_section: "702.113a",
        parameter: Parameter::CountThenMana,
        reminders: &[
            "If you cast this spell for $cost, also put $Nw +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Bestow,
        lexeme: "bestow",
        rules_section: "702.103a",
        parameter: Parameter::Mana,
        reminders: &[
            "If you cast this card for its bestow cost, it's an Aura spell with enchant creature. It becomes a creature again if it's not attached.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Blitz,
        lexeme: "blitz",
        rules_section: "702.152a",
        parameter: Parameter::ManaOrCostSequence,
        reminders: &[
            "If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Conspire,
        lexeme: "conspire",
        rules_section: "702.78a",
        parameter: Parameter::None,
        reminders: &[
            "As you cast this spell, you may tap two untapped creatures you control that share a color with it. When you do, copy it and you may choose a new target for the copy.",
            "As you cast this spell, you may tap two untapped creatures you control that share a color with it. When you do, copy it.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Disturb,
        lexeme: "disturb",
        rules_section: "702.146a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this card from your graveyard transformed for its disturb cost.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Emerge,
        lexeme: "emerge",
        rules_section: "702.119a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this spell by sacrificing a creature and paying the emerge cost reduced by that creature's mana value.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Entwine,
        lexeme: "entwine",
        rules_section: "702.42a",
        parameter: Parameter::ManaOrCostSequence,
        reminders: &[
            "Choose both if you pay the entwine cost.",
            "Choose all if you pay the entwine cost.",
            "Choose all of them if you pay the entwine cost.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Escalate,
        lexeme: "escalate",
        rules_section: "702.120a",
        parameter: Parameter::ManaOrCostSequence,
        reminders: &["Pay this cost for each mode chosen beyond the first."],
    },
    Definition {
        keyword: CastingKeyword::Escape,
        lexeme: "escape",
        rules_section: "702.138a",
        parameter: Parameter::CostSequence,
        reminders: &["You may cast this card from your graveyard for its escape cost."],
    },
    Definition {
        keyword: CastingKeyword::GiftACard,
        lexeme: "gift a card",
        rules_section: "702.174a",
        parameter: Parameter::None,
        reminders: &[
            "You may promise an opponent a gift as you cast this spell. If you do, they draw a card before its other effects.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Gravestorm,
        lexeme: "gravestorm",
        rules_section: "702.69a",
        parameter: Parameter::None,
        reminders: &[
            "When you cast this spell, copy it for each permanent put into a graveyard from the battlefield this turn.",
            "When you cast this spell, copy it for each permanent put into a graveyard from the battlefield this turn. You may choose new targets for the copies.",
            "When you cast this spell, copy it for each permanent put into a graveyard this turn. You may choose new targets for the copies.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Impending,
        lexeme: "impending",
        rules_section: "702.176a",
        parameter: Parameter::CountThenMana,
        reminders: &[
            "If you cast this spell for its impending cost, it enters with $Nw time counters and isn't a creature until the last is removed. At the beginning of your end step, remove a time counter from it.",
        ],
    },
    Definition {
        keyword: CastingKeyword::JumpStart,
        lexeme: "jump-start",
        rules_section: "702.133a",
        parameter: Parameter::None,
        reminders: &[
            "You may cast this card from your graveyard by discarding a card in addition to paying its other costs. Then exile this card.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Kicker,
        lexeme: "kicker",
        rules_section: "702.33a",
        parameter: Parameter::CostSequence,
        reminders: &[],
    },
    Definition {
        keyword: CastingKeyword::Madness,
        lexeme: "madness",
        rules_section: "702.35a",
        parameter: Parameter::ManaOrCostSequence,
        reminders: &[
            "If you discard this card, discard it into exile. When you do, cast it for its madness cost or put it into your graveyard.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Mayhem,
        lexeme: "mayhem",
        rules_section: "702.187b",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this card from your graveyard for $cost if you discarded it this turn. Timing rules still apply.",
            "You may cast this card from your graveyard for $cost if you discarded it this turn.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Paradigm,
        lexeme: "paradigm",
        rules_section: "702.192a",
        parameter: Parameter::None,
        reminders: &[
            "Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Prototype,
        lexeme: "prototype",
        rules_section: "702.160a",
        parameter: Parameter::ManaThenCharacteristics,
        reminders: &[
            "You may cast this spell with different mana cost, color, and size. It keeps its abilities and types.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Retrace,
        lexeme: "retrace",
        rules_section: "702.81a",
        parameter: Parameter::None,
        reminders: &[
            "You may cast this card from your graveyard by discarding a land card in addition to paying its other costs.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Sneak,
        lexeme: "sneak",
        rules_section: "702.190a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this spell for $cost if you also return an unblocked attacker you control to hand during the declare blockers step.",
            "You may cast this spell for $cost if you also return an unblocked attacker you control to hand during the declare blockers step. He enters tapped and attacking.",
            "You may cast this spell for $cost if you also return an unblocked attacker you control to hand during the declare blockers step. She enters tapped and attacking.",
            "You may cast this spell for $cost if you also return an unblocked attacker you control to hand during the declare blockers step. It enters tapped and attacking.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Spectacle,
        lexeme: "spectacle",
        rules_section: "702.137a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this spell for its spectacle cost rather than its mana cost if an opponent lost life this turn.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Surge,
        lexeme: "surge",
        rules_section: "702.117a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this spell for its surge cost if you or a teammate has cast another spell this turn.",
        ],
    },
    Definition {
        keyword: CastingKeyword::Teamwork,
        lexeme: "teamwork",
        rules_section: "702.194a",
        parameter: Parameter::Count,
        reminders: &[
            "As an additional cost to cast this spell, you may tap any number of creatures you control with total power $N or more.",
        ],
    },
    Definition {
        keyword: CastingKeyword::WebSlinging,
        lexeme: "web-slinging",
        rules_section: "702.188a",
        parameter: Parameter::Mana,
        reminders: &[
            "You may cast this spell for $cost if you also return a tapped creature you control to its owner's hand.",
        ],
    },
];

fn definition(keyword: CastingKeyword) -> &'static Definition {
    DEFINITIONS
        .iter()
        .find(|definition| definition.keyword == keyword)
        .expect("every casting keyword has one registered row")
}

/// The lexeme of each row as shared tokens (`web-slinging` is one word).
fn lexemes() -> &'static [(&'static Definition, Vec<String>)] {
    static LEXEMES: std::sync::OnceLock<Vec<(&'static Definition, Vec<String>)>> =
        std::sync::OnceLock::new();
    LEXEMES.get_or_init(|| {
        DEFINITIONS
            .iter()
            .map(|definition| {
                (
                    definition,
                    recognize_entities(definition.lexeme)
                        .into_iter()
                        .map(|token| token.raw.to_ascii_lowercase())
                        .collect(),
                )
            })
            .collect()
    })
}

/// How a keyword's cost parameter is printed: one mana cost, or the shared
/// cost sequence after an em dash (`Escape—{2}{R}, Exile ...`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum KeywordCostNode {
    Mana(ManaCostNode),
    Sequence(ActivationCostSequenceNode),
}

impl KeywordCostNode {
    /// The mana cost when the parameter is exactly one mana payment.
    pub(in crate::oracle::canonical) fn single_mana(&self) -> Option<&ManaCostNode> {
        match self {
            Self::Mana(mana) => Some(mana),
            Self::Sequence(sequence) => match sequence.costs.as_slice() {
                [LocalActivationCostNode::PayMana(mana)] => Some(mana),
                _ => None,
            },
        }
    }
}

/// One typed casting keyword line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastingKeywordNode {
    pub keyword: CastingKeyword,
    pub cost: Option<KeywordCostNode>,
    pub count: Option<i64>,
    /// The prototype power and toughness (CR 702.160a).
    pub power_toughness: Option<(i64, i64)>,
    pub reminder: bool,
}

fn is_count_token(raw: &str) -> bool {
    !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit())
}

fn parameter_starts(parameter: Parameter, next: Option<&RecognizedEntity>) -> bool {
    match parameter {
        Parameter::None => next.is_none_or(|token| token.raw == "("),
        Parameter::Count | Parameter::CountThenMana => {
            next.is_some_and(|token| is_count_token(&token.raw))
        }
        Parameter::Mana | Parameter::ManaThenCharacteristics => {
            next.is_some_and(|token| token.token_type == "manaSymbol")
        }
        Parameter::ManaOrCostSequence => {
            next.is_some_and(|token| token.token_type == "manaSymbol" || token.raw == "\u{2014}")
        }
        Parameter::CostSequence => next.is_some_and(|token| token.raw == "\u{2014}"),
    }
}

/// Structural claim: the ability starts with a casting keyword lexeme
/// immediately followed by the start of that row's parameter.
pub(in crate::oracle::canonical) fn casting_keyword_starts(
    tokens: &[RecognizedEntity],
) -> Option<CastingKeyword> {
    lexemes()
        .iter()
        .find(|(definition, lexeme)| {
            tokens.len() >= lexeme.len()
                && lexeme
                    .iter()
                    .zip(tokens)
                    .all(|(word, token)| token.raw.eq_ignore_ascii_case(word))
                && parameter_starts(definition.parameter, tokens.get(lexeme.len()))
        })
        .map(|(definition, _)| definition.keyword)
}

fn read_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    cursor.enter("keyword.casting.parameter.count");
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => Ok(value),
        _ => Err(cursor.fail("a positive literal keyword count")),
    }
}

fn read_mana(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ManaCostNode> {
    cursor.enter("keyword.casting.parameter.mana-cost");
    let cost = recognize_mana_cost(cursor)?;
    cursor.enter("keyword.casting.parameter.mana-support");
    cost.fixed_runtime_mana_value()
        .map_err(|error| cursor.fail(error.expected()))?;
    Ok(cost)
}

fn read_dash(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.casting.parameter.dash");
    cursor.consume("\u{2014}")
}

/// `—COSTS.`: the shared activation-cost sequence and its terminator.
fn read_cost_sequence(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<KeywordCostNode> {
    read_dash(cursor)?;
    cursor.enter("keyword.casting.parameter.cost-sequence");
    let sequence = recognize_activation_cost_sequence(cursor)?;
    cursor.enter("keyword.casting.parameter.cost-terminator");
    if cursor.peek() == Some(".") {
        cursor.consume(".")?;
    }
    Ok(KeywordCostNode::Sequence(sequence))
}

/// `P/T` printed as one token (`3/3`), each part a literal.
fn read_power_toughness(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<(i64, i64)> {
    cursor.enter("keyword.casting.parameter.power-toughness");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a printed power/toughness"))?
        .to_string();
    let parsed = raw.split_once('/').and_then(|(power, toughness)| {
        Some((power.parse::<i64>().ok()?, toughness.parse::<i64>().ok()?))
    });
    let (power, toughness) =
        cursor.require_value(parsed, "a literal power/toughness such as `3/3`")?;
    cursor.require(
        power >= 0 && toughness > 0,
        "a nonnegative power and a positive toughness",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok((power, toughness))
}

fn rendering_matches(printed: &[RecognizedEntity], rendering: &str) -> bool {
    let expected = recognize_entities(rendering);
    expected.len() == printed.len()
        && expected.iter().zip(printed).all(|(expected, token)| {
            token
                .raw
                .replace('\u{2019}', "'")
                .eq_ignore_ascii_case(&expected.raw)
        })
}

fn read_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    definition: &Definition,
    node: &CastingKeywordNode,
) -> MachineResult<()> {
    cursor.enter("keyword.casting.reminder.open");
    cursor.consume("(")?;
    let body_start = cursor.position();
    let close = cursor
        .token_slice(body_start, usize::MAX)
        .iter()
        .position(|token| token.raw == ")")
        .map(|offset| body_start + offset);
    let body_end = cursor.require_value(close, "a reminder closing the keyword ability")?;
    if definition.reminders.is_empty() {
        return read_restated_cost_reminder(cursor, node, body_end);
    }
    cursor.enter("keyword.casting.reminder.body");
    let printed = cursor.token_slice(body_start, body_end);
    let arguments = KeywordTemplateArguments {
        count: node.count,
        cost: node
            .cost
            .as_ref()
            .and_then(KeywordCostNode::single_mana)
            .map(ManaCostNode::as_oracle_cost),
        subtypes: Vec::new(),
        threshold: None,
    };
    let matches = definition.reminders.iter().any(|reminder| {
        instantiate(reminder, &arguments)
            .is_ok_and(|rendering| rendering_matches(printed, &rendering))
    });
    cursor.require(
        matches,
        "the keyword's reminder instantiated with the printed parameter",
    )?;
    cursor.advance_to(body_end)?;
    cursor.enter("keyword.casting.reminder.close");
    cursor.consume(")")
}

/// `(You may COSTS in addition to any other costs as you cast this spell.)`:
/// the additional-cost reminder restates the printed payments, which the
/// shared cost sequence reads again; connectors may differ (`, ` / `and`).
fn read_restated_cost_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    node: &CastingKeywordNode,
    body_end: usize,
) -> MachineResult<()> {
    cursor.enter("keyword.casting.reminder.permission");
    cursor.consume_phrase(&["you", "may"])?;
    cursor.enter("keyword.casting.reminder.cost");
    let restated = cursor.bounded(body_end, |cursor| {
        let costs = recognize_activation_cost_sequence(cursor)?;
        cursor.enter("keyword.casting.reminder.additional");
        cursor.consume_phrase(&[
            "in", "addition", "to", "any", "other", "costs", "as", "you", "cast", "this", "spell",
            ".",
        ])?;
        Ok(costs)
    })?;
    cursor.enter("keyword.casting.reminder.cost-match");
    let printed = match &node.cost {
        Some(KeywordCostNode::Sequence(sequence)) => Some(&sequence.costs),
        _ => None,
    };
    cursor.require(
        printed == Some(&restated.costs),
        "a reminder restating the printed keyword cost",
    )?;
    cursor.enter("keyword.casting.reminder.close");
    cursor.consume(")")
}

/// Reads one casting keyword line and its optional validated reminder.
pub(in crate::oracle::canonical) fn recognize_casting_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastingKeywordNode> {
    cursor.enter("keyword.casting.lexeme");
    let start = cursor.position();
    let keyword = cursor.require_value(
        casting_keyword_starts(cursor.token_slice(start, usize::MAX)),
        "a casting keyword followed by its parameter",
    )?;
    let definition = definition(keyword);
    let width = lexemes()
        .iter()
        .find(|(row, _)| row.keyword == keyword)
        .map_or(1, |(_, lexeme)| lexeme.len());
    cursor.advance_to(start + width)?;
    let mut node = CastingKeywordNode {
        keyword,
        cost: None,
        count: None,
        power_toughness: None,
        reminder: false,
    };
    cursor.enter("keyword.casting.parameter");
    match definition.parameter {
        Parameter::None => {}
        Parameter::Count => node.count = Some(read_count(cursor)?),
        Parameter::Mana => node.cost = Some(KeywordCostNode::Mana(read_mana(cursor)?)),
        Parameter::ManaOrCostSequence => {
            node.cost = Some(if cursor.peek() == Some("\u{2014}") {
                read_cost_sequence(cursor)?
            } else {
                KeywordCostNode::Mana(read_mana(cursor)?)
            });
        }
        Parameter::CostSequence => node.cost = Some(read_cost_sequence(cursor)?),
        Parameter::CountThenMana => {
            node.count = Some(read_count(cursor)?);
            read_dash(cursor)?;
            node.cost = Some(KeywordCostNode::Mana(read_mana(cursor)?));
        }
        Parameter::ManaThenCharacteristics => {
            node.cost = Some(KeywordCostNode::Mana(read_mana(cursor)?));
            read_dash(cursor)?;
            node.power_toughness = Some(read_power_toughness(cursor)?);
        }
    }
    cursor.enter("keyword.casting.reminder-or-complete");
    if cursor.peek() == Some(".") && cursor.peek_ahead(1).is_none_or(|raw| raw == "(") {
        cursor.consume(".")?;
    }
    if cursor.peek() == Some("(") {
        read_reminder(cursor, definition, &node)?;
        node.reminder = true;
    }
    cursor.enter("keyword.casting.complete");
    cursor.finish()?;
    Ok(node)
}

#[cfg(test)]
#[path = "casting_keyword_tests.rs"]
mod tests;
