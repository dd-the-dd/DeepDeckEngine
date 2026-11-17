use crate::card_catalog::named_token_printing;
use crate::oracle::syntax::recognize_entities;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, NumericExpression, TargetBinding,
    compile_named_token_catalog_rules, recognize_numeric_expression,
};
use std::sync::OnceLock;

/// The keyword action's own name is also the identity of the token it uses
/// (CR 701.71a). The token's rules are read from the shared token catalog.
const EMPOWER_JACE_TOKEN: &str = "Jace";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EmpowerJaceEffectNode {
    pub player: LocalPlayerReference,
    pub count: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmpowerJaceState {
    Action,
    Count,
    Complete,
}

/// True when every printed ability of the token compiles through the shared
/// grammar. A token created without one of its printed abilities would be an
/// approximation, so no partial token definition is accepted.
pub(super) fn token_definition_is_complete(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> bool {
    compile_named_token_catalog_rules(token_name, type_line, oracle).is_some()
}

fn catalog_token_definition_is_complete() -> bool {
    static COMPLETE: OnceLock<bool> = OnceLock::new();
    *COMPLETE.get_or_init(|| {
        named_token_printing(EMPOWER_JACE_TOKEN)
            .ok()
            .flatten()
            .and_then(|printing| {
                Some(token_definition_is_complete(
                    EMPOWER_JACE_TOKEN,
                    printing.type_line.as_deref()?,
                    printing.oracle_text.as_deref()?,
                ))
            })
            .unwrap_or(false)
    })
}

/// Recognizes the keyword action `empower Jace N` (CR 701.71a).
///
/// The action creates the catalog Jace token when needed, so it is accepted
/// only while that token's printed abilities all compile (explicit failure
/// state `empower.token.definition` otherwise). The count is the shared
/// numeric expression. A trailing `where X is ...` definition and the
/// reminder text remain owned by the enclosing frames; the reminder is
/// validated by `recognize_empower_jace_reminder`.
pub(in crate::oracle::canonical) fn recognize_empower_jace_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<EmpowerJaceEffectNode> {
    let node = recognize_empower_jace_action(cursor, antecedent)?;
    cursor.enter("empower.token.definition");
    cursor.require(
        catalog_token_definition_is_complete(),
        "a catalog Jace token whose printed loyalty abilities all compile through the shared grammar",
    )?;
    Ok(node)
}

/// The action syntax alone, without the token-definition gate.
fn recognize_empower_jace_action(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<EmpowerJaceEffectNode> {
    let mut state = EmpowerJaceState::Action;
    let mut count = None;
    loop {
        state = match state {
            EmpowerJaceState::Action => {
                cursor.enter("effect.empower-jace.action");
                cursor.consume("empower")?;
                cursor.consume("jace")?;
                EmpowerJaceState::Count
            }
            EmpowerJaceState::Count => {
                cursor.enter("effect.empower-jace.count");
                let expression = recognize_numeric_expression(cursor, antecedent)?;
                cursor.require(
                    !matches!(
                        &expression,
                        NumericExpression::Atom(NumericAtom::Integer(value)) if *value < 0
                    ),
                    "a nonnegative empower Jace count or variable expression",
                )?;
                count = Some(expression);
                EmpowerJaceState::Complete
            }
            EmpowerJaceState::Complete => {
                cursor.enter("effect.empower-jace.complete");
                return Ok(EmpowerJaceEffectNode {
                    player: LocalPlayerReference::Controller,
                    count: cursor.require_value(count, "an empower Jace count")?,
                });
            }
        };
    }
}

/// Raw lowercase words of one ability line, without loyalty-cost brackets,
/// parenthetical reminder text or the final sentence period.
fn ability_words(tokens: &[crate::oracle::model::RecognizedEntity]) -> Vec<String> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    for token in tokens {
        match token.raw.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "[" | "]" => {}
            _ if depth == 0 => words.push(token.raw.to_lowercase()),
            _ => {}
        }
    }
    while words.last().is_some_and(|word| word == ".") {
        words.pop();
    }
    words
}

fn catalog_token_abilities(cursor: &OracleTokenCursor<'_>) -> MachineResult<Vec<Vec<String>>> {
    let printing = named_token_printing(EMPOWER_JACE_TOKEN)
        .ok()
        .flatten()
        .ok_or_else(|| cursor.fail("a catalog-backed Jace planeswalker token definition"))?;
    let type_line = printing.type_line.as_deref().unwrap_or_default();
    cursor.require(
        type_line.to_ascii_lowercase().contains("planeswalker")
            && type_line.to_ascii_lowercase().contains("jace"),
        "a catalog Jace token with the planeswalker type",
    )?;
    let oracle = printing
        .oracle_text
        .as_deref()
        .ok_or_else(|| cursor.fail("catalog Jace token Oracle"))?;
    Ok(oracle
        .lines()
        .map(|line| ability_words(&recognize_entities(line)))
        .filter(|words| !words.is_empty())
        .collect())
}

/// Validates the printed reminder of `empower Jace N`: the counter quantity
/// repeats the action count, and the created token description matches the
/// catalog token's loyalty abilities line by line.
pub(in crate::oracle::canonical) fn recognize_empower_jace_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    let expected = catalog_token_abilities(cursor)?;
    cursor.enter("effect.empower-jace.reminder.open");
    cursor.consume("(")?;
    cursor.consume("put")?;
    cursor.enter("effect.empower-jace.reminder.count");
    match count {
        NumericExpression::Atom(NumericAtom::Integer(1)) => cursor.consume("a")?,
        NumericExpression::Atom(NumericAtom::Integer(_)) => {
            let repeated = recognize_numeric_expression(cursor, None)?;
            cursor.require(
                repeated == *count,
                "the same literal count as the empower Jace action",
            )?;
        }
        _ => cursor.consume_phrase(&["that", "many"])?,
    }
    let counter_word = if matches!(count, NumericExpression::Atom(NumericAtom::Integer(1))) {
        "counter"
    } else {
        "counters"
    };
    cursor.consume("loyalty")?;
    cursor.consume(counter_word)?;
    cursor.consume_phrase(&["on", "a", "jace", "token", "you", "control", "."])?;
    cursor.enter("effect.empower-jace.reminder.creation");
    cursor.consume_phrase(&[
        "if",
        "you",
        "don't",
        "control",
        "one",
        ",",
        "first",
        "create",
        "a",
        "blue",
        "jace",
        "planeswalker",
        "token",
        "with",
    ])?;
    cursor.enter("effect.empower-jace.reminder.abilities");
    let mut described = Vec::new();
    loop {
        cursor.consume("\"")?;
        let start = cursor.position();
        while cursor.peek().is_some_and(|raw| raw != "\"") {
            cursor.advance_to(cursor.position() + 1)?;
        }
        let words = ability_words(cursor.token_slice(start, cursor.position()));
        cursor.consume("\"")?;
        described.push(words);
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        {
            cursor.consume("and")?;
        } else {
            break;
        }
    }
    cursor.require(
        described == expected,
        "token abilities identical to the catalog Jace token",
    )?;
    cursor.enter("effect.empower-jace.reminder.close");
    cursor.consume(")")?;
    Ok(())
}

#[cfg(test)]
#[path = "empower_jace_tests.rs"]
mod tests;
