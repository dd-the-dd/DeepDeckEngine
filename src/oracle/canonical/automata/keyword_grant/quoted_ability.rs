//! A quoted ability granted to an object (CR 113.1, 604.1):
//!
//! ```text
//! KeywordList   := Atom (("," ["and"] | "and") Atom)*      (shared keyword list)
//! Atom          := keyword | QuotedAbility
//! QuotedAbility := '"' <one complete ability of the granted object> '"'
//! ```
//!
//! The quoted text is an ability of the object that gains it, so it is compiled
//! by the ordinary ability pipeline as a face of its own: `this creature` inside
//! the quote is the granted object, never the granting source. The child is
//! all-or-nothing; a quote with any non-canonical ability is a failure that
//! carries the inner diagnostic.
use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::source_text;
use serde_json::Value;

/// One complete compiled ability of the granted object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuotedAbilityNode {
    pub rule: Value,
}

/// Type line used to compile the quote when the grant does not fix a narrower
/// one. A granted ability belongs to a permanent; the printed noun inside the
/// quote still has to agree with it.
pub(in crate::oracle::canonical) const GRANTED_OBJECT_TYPE_LINE: &str = "Creature";

/// Compiles the quote opening at the cursor and consumes both quotation marks.
pub(in crate::oracle::canonical) fn recognize_quoted_ability(
    cursor: &mut OracleTokenCursor<'_>,
    granted_type_line: &str,
) -> MachineResult<QuotedAbilityNode> {
    cursor.enter("quoted-ability.open");
    cursor.consume("\"")?;
    let start = cursor.position();
    let mut length = 0;
    while cursor.peek_ahead(length).is_some_and(|word| word != "\"") {
        length += 1;
    }
    cursor.enter("quoted-ability.close");
    cursor.require(
        cursor.peek_ahead(length) == Some("\""),
        "a closing quotation mark after one granted ability",
    )?;
    let mut end = start + length;
    // American punctuation places a list comma inside the closing quote.
    let trailing_comma = end > start + 1 && cursor.token_slice(start, end)[length - 1].raw == ",";
    if trailing_comma {
        end -= 1;
    }
    cursor.enter("quoted-ability.body");
    cursor.require(end > start, "a granted ability inside the quotation marks")?;
    let mut text = source_text(cursor.token_slice(start, end));
    if !text.ends_with('.') {
        text.push('.');
    }
    let result = crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: "Quoted Ability Holder".to_string(),
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
                "one complete canonical granted ability (inner: {inner})"
            )));
        }
        _ => {
            return Err(cursor.fail("exactly one ability inside the quotation marks"));
        }
    };
    cursor.advance_to(start + length)?;
    cursor.consume("\"")?;
    Ok(QuotedAbilityNode { rule })
}

/// A quoted ability that ends with its own period closes the enclosing
/// sentence as well (`gains "...draw a card."`): the printed period sits inside
/// the quotation marks, so a parent that would consume a sentence terminator
/// must treat that quote as the terminator instead of requiring a second one.
pub(in crate::oracle::canonical) fn quoted_sentence_just_closed(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let position = cursor.position();
    position >= 2 && {
        let previous = cursor.token_slice(position - 2, position);
        previous[0].raw == "." && previous[1].raw == "\""
    }
}
