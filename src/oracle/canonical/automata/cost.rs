use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericAtom, NumericExpression, ObjectCardType, TapSelfCostNode};

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

/// The zone an exile-self cost names as the source's location. Only the
/// graveyard is representable: the cost proves the ability is activated there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ExileSelfSourceZone {
    Graveyard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExileSelfCostNode {
    pub source_zone: ExileSelfSourceZone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaCostNode {
    symbols: Vec<String>,
}

/// A preserved cost may be lexically valid but outside the executable fixed-cost subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum FixedRuntimeManaCostError {
    Variable,
    UnsupportedSymbol,
    AmountOutOfRange,
}

impl FixedRuntimeManaCostError {
    pub(in crate::oracle::canonical) fn expected(self) -> &'static str {
        match self {
            Self::Variable => {
                "a fixed mana cost; variable Flashback payment requires an explicit casting-X producer and runtime proof"
            }
            Self::UnsupportedSymbol => {
                "supported whole-mana symbols: generic, colored, colorless, snow, two-color hybrid, or colored Phyrexian; fractional mana payments are not yet supported"
            }
            Self::AmountOutOfRange => {
                "a fixed mana cost whose complete mana value fits a nonnegative i32"
            }
        }
    }
}

impl ManaCostNode {
    /// Checks the retained typed symbols without rebuilding or reparsing Oracle text.
    /// This bounded capability leaves the lexical cost reader and its other callers unchanged.
    pub(in crate::oracle::canonical) fn fixed_runtime_mana_value(
        &self,
    ) -> Result<i32, FixedRuntimeManaCostError> {
        use FixedRuntimeManaCostError::{AmountOutOfRange, UnsupportedSymbol, Variable};
        let mut total = 0_i32;
        for raw in &self.symbols {
            let symbol = raw
                .strip_prefix('{')
                .and_then(|symbol| symbol.strip_suffix('}'))
                .ok_or(UnsupportedSymbol)?;
            let symbol = symbol.to_ascii_uppercase();
            let amount = if symbol == "X" {
                return Err(Variable);
            } else if !symbol.is_empty() && symbol.bytes().all(|byte| byte.is_ascii_digit()) {
                symbol.parse::<i32>().map_err(|_| AmountOutOfRange)?
            } else if matches!(symbol.as_str(), "W" | "U" | "B" | "R" | "G" | "C" | "S") {
                1
            } else {
                let bytes = symbol.as_bytes();
                let is_color = |byte: u8| matches!(byte, b'W' | b'U' | b'B' | b'R' | b'G');
                if bytes.len() == 3
                    && bytes[1] == b'/'
                    && is_color(bytes[0])
                    && (is_color(bytes[2]) || bytes[2] == b'P')
                {
                    1
                } else {
                    return Err(UnsupportedSymbol);
                }
            };
            total = total.checked_add(amount).ok_or(AmountOutOfRange)?;
        }
        Ok(total)
    }

    pub(in crate::oracle::canonical) fn as_oracle_cost(&self) -> String {
        self.symbols.concat()
    }

    /// The one whole-mana symbol (`W`, `U`, `B`, `R`, `G` or `C`) this cost
    /// repeats, with its multiplicity. Mixed or non-whole symbols are `None`.
    pub(in crate::oracle::canonical) fn repeated_mana_symbol(&self) -> Option<(char, usize)> {
        let first = self.symbols.first()?;
        let symbol = first.strip_prefix('{')?.strip_suffix('}')?;
        let mut letters = symbol.chars();
        let letter = letters.next()?.to_ascii_uppercase();
        if letters.next().is_some() || !matches!(letter, 'W' | 'U' | 'B' | 'R' | 'G' | 'C') {
            return None;
        }
        self.symbols
            .iter()
            .all(|raw| raw.eq_ignore_ascii_case(first))
            .then_some((letter, self.symbols.len()))
    }

    pub(super) fn same_symbols(&self, other: &Self) -> bool {
        self.symbols.len() == other.symbols.len()
            && self
                .symbols
                .iter()
                .zip(&other.symbols)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    }

    /// Projects generic and X mana symbols into one typed numeric expression.
    /// Colored symbols require a separate fixed component in a wider cost program.
    pub(in crate::oracle::canonical) fn generic_payment_expression(
        &self,
    ) -> Option<NumericExpression> {
        let mut fixed = 0_i32;
        let mut x_count = 0_i32;
        for raw in &self.symbols {
            let symbol = raw.strip_prefix('{')?.strip_suffix('}')?;
            if symbol.eq_ignore_ascii_case("X") {
                x_count = x_count.checked_add(1)?;
            } else if !symbol.is_empty() && symbol.bytes().all(|byte| byte.is_ascii_digit()) {
                fixed = fixed.checked_add(symbol.parse::<i32>().ok()?)?;
            } else {
                return None;
            }
        }
        let constant = NumericExpression::Atom(NumericAtom::Integer(i64::from(fixed)));
        if x_count == 0 {
            return Some(constant);
        }
        let variable = if x_count == 1 {
            NumericExpression::Atom(NumericAtom::VariableX)
        } else {
            NumericExpression::multiply(
                NumericExpression::Atom(NumericAtom::Integer(i64::from(x_count))),
                NumericExpression::Atom(NumericAtom::VariableX),
            )
        };
        Some(if fixed == 0 {
            variable
        } else {
            NumericExpression::add(constant, variable)
        })
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

#[cfg(test)]
#[path = "cost_fixed_generic_tests.rs"]
mod fixed_generic_tests;

#[cfg(test)]
#[path = "cost_fixed_runtime_tests.rs"]
mod fixed_runtime_tests;

#[cfg(test)]
#[path = "cost_payment_expression_tests.rs"]
mod payment_expression_tests;
