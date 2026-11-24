//! A lexical spending qualifier on a single typed mana-production result.
//!
//! ```text
//! Policy := "Spend this mana only" USE {("or" | ", or") ["to"] USE}
//!         | "This mana" ("can't" | "cannot") "be spent to cast" SPELLS
//! USE    := "to cast" [a|an] SPELLS
//!         | "to activate" ("abilities" | "an ability") ["of" [a|an] SOURCES]
//! ```
//!
//! The uses form a union (CR 106.6): the mana may be spent on any of them.
//! Each use reads its object with the shared object query.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// One permitted use of restricted mana (CR 106.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaSpendUse {
    /// `cast SPELLS`.
    CastSpell(ObjectPredicate),
    /// `activate abilities [of SOURCES]`: `None` is any ability's source.
    ActivateAbility { source: Option<ObjectPredicate> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendingPolicyNode {
    /// The spells of the cast use.
    pub spells: ObjectPredicate,
    /// Every permitted use, in Oracle order; the cast use included.
    pub uses: Vec<ManaSpendUse>,
    pub spell_cant_be_countered: bool,
    /// `This mana can't be spent to cast SPELLS` (CR 106.6): the spells are
    /// the uses the mana is barred from, not the only permitted ones.
    pub prohibited: bool,
}

impl ManaSpendingPolicyNode {
    /// Whether abilities of the same objects as the cast spells are also a
    /// permitted use (`cast artifact spells or activate abilities of
    /// artifacts`). The recognizer admits no other ability use.
    pub(in crate::oracle::canonical) fn also_activates_abilities(&self) -> bool {
        self.uses
            .iter()
            .any(|spend_use| matches!(spend_use, ManaSpendUse::ActivateAbility { .. }))
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn consume_optional_article(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        let article = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&article)?;
    }
    Ok(())
}

/// The spells of a cast use, read by the shared object query.
fn recognize_spell_query(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    consume_optional_article(cursor)?;
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
    Ok(query.predicate)
}

/// `activate abilities [of SOURCES]` / `activate an ability [of a SOURCE]`.
fn recognize_activate_use(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ManaSpendUse> {
    cursor.enter("mana.policy.activate-use");
    cursor.consume("activate")?;
    if word_at(cursor, 0, "abilities") {
        cursor.consume("abilities")?;
    } else {
        cursor.consume_phrase(&["an", "ability"])?;
    }
    if !word_at(cursor, 0, "of") {
        return Ok(ManaSpendUse::ActivateAbility { source: None });
    }
    cursor.enter("mana.policy.ability-source");
    cursor.consume("of")?;
    consume_optional_article(cursor)?;
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none()
            && !query.exclude_source
            && !object_predicate_uses_x(&query.predicate)
            && crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
        "an ability-source predicate without a controller or numeric binding",
    )?;
    Ok(ManaSpendUse::ActivateAbility {
        source: Some(query.predicate),
    })
}

/// The width of a connector before a further use: `or` / `, or`, with an
/// optional repeated `to`.
fn further_use_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut offset = 0;
    if cursor.peek_ahead(offset) == Some(",") {
        offset += 1;
    }
    if !word_at(cursor, offset, "or") {
        return None;
    }
    offset += 1;
    if word_at(cursor, offset, "to") {
        offset += 1;
    }
    (word_at(cursor, offset, "cast") || word_at(cursor, offset, "activate")).then_some(offset)
}

/// CR 106.6: the runtime spends restricted mana on cast spells, or on cast
/// spells and abilities of the same objects. Another union fails here.
fn validate_runtime_union(
    cursor: &OracleTokenCursor<'_>,
    uses: &[ManaSpendUse],
) -> MachineResult<ObjectPredicate> {
    let mut spells = None;
    let mut sources = Vec::new();
    for spend_use in uses {
        match spend_use {
            ManaSpendUse::CastSpell(predicate) => {
                cursor.require(spells.is_none(), "one cast use in a spending union")?;
                spells = Some(predicate.clone());
            }
            ManaSpendUse::ActivateAbility { source } => sources.push(source.clone()),
        }
    }
    let spells = cursor.require_value(
        spells,
        "a cast use (the runtime restricts mana to cast spells, with their abilities)",
    )?;
    cursor.require(sources.len() <= 1, "one ability use in a spending union")?;
    if let Some(source) = sources.first() {
        let source = cursor.require_value(
            source.as_ref(),
            "abilities of the same objects as the cast spells (any ability has no runtime restriction)",
        )?;
        let spell_filter = crate::oracle::canonical::effects::lower_spell_object_predicate(&spells);
        let source_filter =
            crate::oracle::canonical::effects::lower_permanent_object_predicate(source);
        cursor.require(
            !source_filter.is_null() && spell_filter.as_ref() == Some(&source_filter),
            "abilities of the same objects as the cast spells (CR 106.6)",
        )?;
    }
    Ok(spells)
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
        cursor.consume_phrase(&["only", "to"])?;
        cursor.require(
            word_at(cursor, 0, "cast") || word_at(cursor, 0, "activate"),
            "a cast or activation use",
        )?;
    }
    let mut uses = Vec::new();
    if prohibited {
        // The barred use states its spells right after `to cast`.
        uses.push(ManaSpendUse::CastSpell(recognize_spell_query(cursor)?));
    } else {
        loop {
            uses.push(if word_at(cursor, 0, "activate") {
                recognize_activate_use(cursor)?
            } else {
                cursor.enter("mana.policy.cast-use");
                cursor.consume("cast")?;
                ManaSpendUse::CastSpell(recognize_spell_query(cursor)?)
            });
            let Some(width) = further_use_width(cursor) else {
                break;
            };
            cursor.enter("mana.policy.use-connector");
            cursor.advance_to(cursor.position() + width)?;
        }
    }
    cursor.enter("mana.policy.runtime-union");
    let spells = validate_runtime_union(cursor, &uses)?;
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
    cursor.require(
        !(spell_cant_be_countered && uses.len() > 1),
        "an uncounterable consequence of a single cast use",
    )?;
    Ok(ManaSpendingPolicyNode {
        spells,
        uses,
        spell_cant_be_countered,
        prohibited,
    })
}

#[cfg(test)]
#[path = "mana_spending_policy_tests.rs"]
mod tests;
