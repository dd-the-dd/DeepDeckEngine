//! A lexical spending qualifier on a single typed mana-production result.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendingPolicyNode {
    pub spells: ObjectPredicate,
    pub spell_cant_be_countered: bool,
}

pub(in crate::oracle::canonical) fn recognize_mana_spending_policy(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpendingPolicyNode> {
    cursor.enter("mana.policy.produced-reference");
    cursor.consume_phrase(&["spend", "this", "mana"])?;
    cursor.enter("mana.policy.use");
    cursor.consume_phrase(&["only", "to", "cast"])?;
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
            && query.predicate.has_domain(ObjectKind::Spell)
            && query.predicate.supports_domain(ObjectKind::Spell)
            && !object_predicate_uses_x(&query.predicate),
        "a spell-domain spending predicate without external numeric bindings",
    )?;
    cursor.enter("mana.policy.predicate-capability");
    cursor.require(
        crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
        "a spell predicate preserving Boolean domain identities",
    )?;
    let spell_cant_be_countered = if cursor.peek() == Some(",") {
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
    })
}

#[cfg(test)]
#[path = "mana_spending_policy_tests.rs"]
mod tests;
