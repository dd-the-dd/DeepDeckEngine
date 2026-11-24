use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, NumericExpression, ObjectControllerScope, ObjectKind, ObjectQueryNode,
    TapSelfCostNode, recognize_numeric_atom, recognize_object_query,
};

/// `sacrifice this PERMANENT`: the shared source reference proved the noun
/// names the source permanent; lowering pays the cost with the source itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SacrificeSelfCostNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DiscardSelfCostNode;

/// The zone an exile-self cost pays the source from. A card zone named by the
/// cost proves the ability is activated there; a source permanent
/// (`exile this artifact`) is exiled from the battlefield.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ExileSelfSourceZone {
    Graveyard,
    Battlefield,
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

/// `tap any number of [other] [untapped] QUERY with total power N or greater`
/// (Crew CR 702.122a, Saddle CR 702.171a, printed tap costs): untapped
/// permanents the payer controls, chosen while paying, whose powers sum to
/// at least N. The quantity is not a count but an aggregate bound on the
/// chosen set; the cost parent projects it onto its own schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TotalPowerTapCostNode {
    pub object: ObjectQueryNode,
    /// `other`: the ability's source cannot be one of the tapped permanents.
    pub exclude_source: bool,
    pub minimum_power: i64,
}

/// Lookahead only: the `any number of` quantity of a total-power tap cost.
pub(in crate::oracle::canonical) fn total_power_tap_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("any"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("number"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
}

/// The first `with total power` after the cursor, which ends the shared
/// object query of a total-power tap cost.
fn total_power_boundary(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    tokens
        .windows(3)
        .position(|window| {
            window[0].raw.eq_ignore_ascii_case("with")
                && window[1].raw.eq_ignore_ascii_case("total")
                && window[2].raw.eq_ignore_ascii_case("power")
        })
        .map(|offset| cursor.position() + offset)
}

/// Reads the operand of `tap` after the verb:
///
/// ```text
/// "any" "number" "of" ["other"] ["untapped"] QUERY
///     "with" "total" "power" N "or" ("greater" | "more")
/// ```
///
/// The query is the shared object query, bounded by `with total power`; it
/// must name permanents the payer controls (`you control`). N is a positive
/// literal: a variable bound would need a producer this cost has not.
pub(in crate::oracle::canonical) fn recognize_total_power_tap_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TotalPowerTapCostNode> {
    cursor.enter("cost.total-power-tap.quantity");
    cursor.consume_phrase(&["any", "number", "of"])?;
    let exclude_source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
    if exclude_source {
        cursor.consume("other")?;
    }
    // Only untapped permanents can be tapped (CR 701.26a): the written
    // status is redundant evidence of the same requirement.
    cursor.enter("cost.total-power-tap.status");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("untapped"))
    {
        cursor.consume("untapped")?;
    }
    cursor.enter("cost.total-power-tap.object-query");
    let boundary = cursor.require_value(
        total_power_boundary(cursor),
        "`with total power N or greater` after the tapped permanents",
    )?;
    let object = cursor.bounded(boundary, recognize_object_query)?;
    cursor.enter("cost.total-power-tap.domain");
    cursor.require(
        object.controller == Some(ObjectControllerScope::AbilityController)
            && !object.exclude_source
            && object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "battlefield permanents with a nominal head controlled by the payer (`you control`)",
    )?;
    cursor.enter("cost.total-power-tap.aggregate");
    cursor.consume_phrase(&["with", "total", "power"])?;
    cursor.enter("cost.total-power-tap.minimum");
    let minimum_power = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => value,
        _ => return Err(cursor.fail("a positive literal total power")),
    };
    cursor.enter("cost.total-power-tap.comparison");
    cursor.consume("or")?;
    cursor.require(
        cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("greater") || raw.eq_ignore_ascii_case("more")
        }),
        "`or greater` or `or more` (a lower bound on the total power)",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("cost.total-power-tap.complete");
    Ok(TotalPowerTapCostNode {
        object,
        exclude_source,
        minimum_power,
    })
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
