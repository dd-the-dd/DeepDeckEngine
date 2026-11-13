use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};

use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    DiscardVerbForm, recognize_discard_action, recognize_sacrifice_action,
};
use super::{ObjectCardType, TapSelfCostNode};

/// A source-sacrifice cost whose Oracle noun explicitly confirms the source's
/// permanent type or that the source is a token. The noun is retained as typed
/// syntax even though lowering pays the cost with the source reference itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeSelfSourceNoun {
    PermanentType(ObjectCardType),
    Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SacrificeSelfCostNode {
    pub source_type: SacrificeSelfSourceNoun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DiscardSelfCostNode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaCostNode {
    symbols: Vec<String>,
}

impl ManaCostNode {
    pub(in crate::oracle::canonical) fn as_oracle_cost(&self) -> String {
        self.symbols.concat()
    }

    pub(super) fn same_symbols(&self, other: &Self) -> bool {
        self.symbols.len() == other.symbols.len()
            && self
                .symbols
                .iter()
                .zip(&other.symbols)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    }

    pub(in crate::oracle::canonical) fn contains_variable_x(&self) -> bool {
        self.symbols
            .iter()
            .any(|symbol| symbol.eq_ignore_ascii_case("{X}"))
    }

    /// Returns the bounded value of exactly one generic mana symbol.
    /// Positivity remains a semantic decision for the parent cost grammar.
    pub(in crate::oracle::canonical) fn fixed_generic_amount(&self) -> Option<i32> {
        let [symbol] = self.symbols.as_slice() else {
            return None;
        };
        let digits = symbol.strip_prefix('{')?.strip_suffix('}')?;
        (!digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()))
            .then(|| digits.parse::<i32>().ok())
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManaCostState {
    Start,
    Symbol,
    Complete,
}

/// Consumes one complete sequence of adjacent mana-symbol tokens.
///
/// Symbol validation belongs to the shared mana contract used by the engine;
/// this lexical child preserves the exact Oracle representation without
/// reconstructing or reparsing the source text.
pub(in crate::oracle::canonical) fn recognize_mana_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaCostNode> {
    recognize_mana_sequence(cursor, "cost.mana", |_| true)
}

/// Consumes fixed mana symbols that can be produced directly. Generic,
/// variable, hybrid, Phyrexian, and snow symbols remain cost concepts until a
/// production grammar defines their meaning explicitly.
pub(in crate::oracle::canonical) fn recognize_fixed_mana_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaCostNode> {
    recognize_mana_sequence(cursor, "mana.fixed", |raw| {
        matches!(
            raw.to_ascii_uppercase().as_str(),
            "{W}" | "{U}" | "{B}" | "{R}" | "{G}" | "{C}"
        )
    })
}

fn recognize_mana_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    state_prefix: &'static str,
    accepts: impl Fn(&str) -> bool,
) -> MachineResult<ManaCostNode> {
    let mut state = ManaCostState::Start;
    let mut symbols = Vec::new();
    loop {
        state = match state {
            ManaCostState::Start => {
                cursor.enter(match state_prefix {
                    "mana.fixed" => "mana.fixed.start",
                    _ => "cost.mana.start",
                });
                if cursor
                    .peek_token()
                    .is_none_or(|token| token.token_type != "manaSymbol")
                {
                    return Err(cursor.fail("one or more mana symbols"));
                }
                ManaCostState::Symbol
            }
            ManaCostState::Symbol => {
                cursor.enter(match state_prefix {
                    "mana.fixed" => "mana.fixed.symbol",
                    _ => "cost.mana.symbol",
                });
                while cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "manaSymbol")
                {
                    cursor.require(
                        cursor.peek().is_some_and(&accepts),
                        "a fixed producible mana symbol ({W}, {U}, {B}, {R}, {G}, or {C})",
                    )?;
                    symbols.push(cursor.consume_type("manaSymbol")?.raw.clone());
                }
                ManaCostState::Complete
            }
            ManaCostState::Complete => {
                cursor.enter(match state_prefix {
                    "mana.fixed" => "mana.fixed.complete",
                    _ => "cost.mana.complete",
                });
                cursor.require(!symbols.is_empty(), "at least one mana symbol")?;
                return Ok(ManaCostNode { symbols });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_tap_self_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TapSelfCostNode> {
    cursor.enter("cost.tap-self.symbol");
    let symbol = cursor.consume_type("tapSymbol")?;
    cursor.require(symbol.raw.eq_ignore_ascii_case("{T}"), "the {T} tap symbol")?;
    cursor.enter("cost.tap-self.complete");
    Ok(TapSelfCostNode)
}

/// Recognizes `Sacrifice this <permanent type|token>` without reconstructing a
/// text slice. The activated-cost parent owns separators and cost sequencing.
pub(in crate::oracle::canonical) fn recognize_sacrifice_self_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SacrificeSelfCostNode> {
    let action = recognize_sacrifice_action(cursor, |cursor| {
        cursor.enter("cost.sacrifice-self.reference");
        cursor.consume("this")?;
        cursor.enter("cost.sacrifice-self.source-type");
        recognize_sacrifice_source_noun(cursor)
    })?;
    Ok(SacrificeSelfCostNode {
        source_type: action.operand,
    })
}

/// Recognizes a source-card discard cost. The activated envelope later marks
/// the source as hand-active; this child owns only the independent cost atom.
pub(in crate::oracle::canonical) fn recognize_discard_self_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DiscardSelfCostNode> {
    recognize_discard_action(cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.enter("cost.discard-self.reference");
        cursor.consume("this")?;
        cursor.enter("cost.discard-self.object");
        cursor.consume("card")?;
        Ok(())
    })?;
    cursor.enter("cost.discard-self.complete");
    Ok(DiscardSelfCostNode)
}

fn recognize_sacrifice_source_noun(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SacrificeSelfSourceNoun> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a permanent type or `token` after `sacrifice this`"))?;
    let noun = if raw.eq_ignore_ascii_case("token") {
        SacrificeSelfSourceNoun::Token
    } else {
        lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
            .map(SacrificeSelfSourceNoun::PermanentType)
            .ok_or_else(|| cursor.fail("a permanent type or `token` after `sacrifice this`"))?
    };
    cursor.advance_to(cursor.position() + 1)?;
    Ok(noun)
}

#[cfg(test)]
#[path = "cost_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cost_fixed_generic_tests.rs"]
mod fixed_generic_tests;
