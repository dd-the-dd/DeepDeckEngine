//! A lexical spending qualifier on a single typed mana-production result.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendingPolicyNode {
    pub spells: ObjectPredicate,
    pub spell_cant_be_countered: bool,
    /// `This mana can't be spent to cast SPELLS` (CR 106.6): the spells are
    /// the uses the mana is barred from, not the only permitted ones.
    pub prohibited: bool,
}

/// Whether a spending sentence about the produced mana starts here: `Spend
/// this mana ...` or `This mana can't|cannot be spent ...`.
pub(in crate::oracle::canonical) fn mana_spending_policy_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "spend")
        || (word(0, "this") && word(1, "mana") && (word(2, "can't") || word(2, "cannot")))
}

pub(in crate::oracle::canonical) fn recognize_mana_spending_policy(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpendingPolicyNode> {
    cursor.enter("mana.policy.produced-reference");
    let prohibited = !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("spend"));
    if prohibited {
        cursor.consume_phrase(&["this", "mana"])?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("cannot"))
        {
            cursor.consume("cannot")?;
        } else {
            cursor.consume("can't")?;
        }
        cursor.enter("mana.policy.prohibited-use");
        cursor.consume_phrase(&["be", "spent", "to", "cast"])?;
    } else {
        cursor.consume_phrase(&["spend", "this", "mana"])?;
        cursor.enter("mana.policy.use");
        cursor.consume_phrase(&["only", "to", "cast"])?;
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&article)?;
    }
    cursor.enter("mana.policy.spell-query");
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none()
            && query.predicate.is_in_domain(ObjectKind::Spell)
            && !object_predicate_uses_x(&query.predicate),
        "a spell-domain spending predicate without external numeric bindings",
    )?;
    cursor.enter("mana.policy.predicate-capability");
    cursor.require(
        crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
        "a spell predicate preserving Boolean domain identities",
    )?;
    let spell_cant_be_countered = if !prohibited && cursor.peek() == Some(",") {
        cursor.enter("mana.policy.spent-spell-consequence");
        cursor.consume(",")?;
        cursor.consume_phrase(&["and", "that", "spell"])?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("cannot"))
        {
            cursor.consume("cannot")?;
        } else {
            cursor.consume("can't")?;
        }
        cursor.consume_phrase(&["be", "countered"])?;
        true
    } else {
        false
    };
    cursor.enter("mana.policy.complete");
    Ok(ManaSpendingPolicyNode {
        spells: query.predicate,
        spell_cant_be_countered,
        prohibited,
    })
}

#[cfg(test)]
#[path = "mana_spending_policy_tests.rs"]
mod tests;
