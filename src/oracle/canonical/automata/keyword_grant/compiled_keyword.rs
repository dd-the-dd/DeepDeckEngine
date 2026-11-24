//! A keyword with a printed parameter granted to another object (CR 702.1,
//! 113.1):
//!
//! ```text
//! GrantedAtom := COUNT-KEYWORD N                 (toxic 1, annihilator 2, afflict 3)
//!              | TEMPLATE-KEYWORD Parameter      (bushido 2, training, outlast {1})
//! Parameter   := ε | N | MANA-COST | N "—" MANA-COST
//! ```
//!
//! The atom names abilities of the object that gains it, exactly like a
//! quoted ability: the keyword's printed span is compiled by the ordinary
//! ability pipeline as one keyword ability of a granted object, so the
//! keyword's identity, its count and its instantiated template definition are
//! the ones a printed keyword line produces. Nothing is re-expanded here. A
//! template keyword yields every rule of its definition (CR 702.1: a keyword
//! may represent several abilities); a count keyword yields its keyword rule.
//! The atom commits at its lexeme: an unreadable parameter or a span the
//! pipeline does not compile canonically fails explicitly.
use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::{
    KeywordTemplateParameter, NumericAtom, grantable_count_keyword_length, recognize_mana_cost,
    recognize_numeric_atom, source_text, template_keyword_atom_head,
};
use super::QuotedAbilityNode;

/// The head of a granted parameterized keyword at the cursor: its lexeme
/// length and how its parameter is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CompiledKeywordHead {
    lexeme: usize,
    parameter: KeywordTemplateParameter,
}

pub(super) fn compiled_keyword_head(cursor: &OracleTokenCursor<'_>) -> Option<CompiledKeywordHead> {
    let remaining = cursor.token_slice(cursor.position(), usize::MAX);
    if let Some(lexeme) = grantable_count_keyword_length(remaining) {
        return Some(CompiledKeywordHead {
            lexeme,
            parameter: KeywordTemplateParameter::Count,
        });
    }
    template_keyword_atom_head(remaining)
        .map(|(lexeme, parameter)| CompiledKeywordHead { lexeme, parameter })
}

fn read_positive_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.granted-parameterized.count");
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => Ok(()),
        _ => Err(cursor.fail("a positive literal count for the granted keyword")),
    }
}

fn read_fixed_mana_cost(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.granted-parameterized.mana-cost");
    let cost = recognize_mana_cost(cursor)?;
    cost.fixed_runtime_mana_value()
        .map_err(|error| cursor.fail(error.expected()))?;
    Ok(())
}

/// Reads the atom at `head` and compiles its printed span as the granted
/// object's keyword ability.
pub(super) fn recognize_compiled_keyword(
    cursor: &mut OracleTokenCursor<'_>,
    head: CompiledKeywordHead,
    granted_type_line: &str,
) -> MachineResult<Vec<QuotedAbilityNode>> {
    cursor.enter("keyword.granted-parameterized.lexeme");
    let start = cursor.position();
    cursor.advance_to(start + head.lexeme)?;
    cursor.enter("keyword.granted-parameterized.parameter");
    match head.parameter {
        KeywordTemplateParameter::None => {}
        KeywordTemplateParameter::Count => read_positive_count(cursor)?,
        KeywordTemplateParameter::ManaCost => read_fixed_mana_cost(cursor)?,
        KeywordTemplateParameter::CountThenManaCost => {
            read_positive_count(cursor)?;
            cursor.consume("\u{2014}")?;
            read_fixed_mana_cost(cursor)?;
        }
    }
    let end = cursor.position();
    cursor.enter("keyword.granted-parameterized.compile");
    let text = source_text(cursor.token_slice(start, end));
    let result = crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: "Granted Keyword Holder".to_string(),
        type_line: granted_type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let rule = match result.abilities.as_slice() {
        [ability] if ability.status == "canonical" && ability.rule.is_some() => {
            ability.rule.clone().expect("checked canonical rule")
        }
        [ability] => {
            let inner = ability
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.as_str())
                .unwrap_or("not a canonical ability");
            return Err(cursor.fail(&format!(
                "one canonical keyword ability of the granted object (inner: {inner})"
            )));
        }
        _ => return Err(cursor.fail("exactly one keyword ability for the granted keyword")),
    };
    // A template keyword compiles to its expansion; the granted object gains
    // every rule of that definition, each keeping the keyword's provenance.
    let rules = crate::oracle::canonical::keyword_expansion_rules(&rule);
    cursor.require(
        !rules.is_empty(),
        "one or more rules for the granted keyword",
    )?;
    Ok(rules
        .into_iter()
        .map(|rule| QuotedAbilityNode { rule })
        .collect())
}
