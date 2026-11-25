//! The riders on a single typed mana-production result: one reader with
//! typed slots, each sentence naming `this mana`.
//!
//! ```text
//! Riders    := Rider {"." Rider}
//! Rider     := Policy | Retention
//! Policy    := "Spend this mana only" USE {("or" | ", or") ["to"] USE}
//!            | "This mana" ("can't" | "cannot") "be spent to cast" SPELLS
//! USE       := "to cast" [a|an] SPELLS
//!            | "to activate" ("abilities" | "an ability") ["of" [a|an] SOURCES]
//! Retention := "Until end of turn," "you" ("don't" | "do not") "lose this
//!              mana as steps and phases end"          (CR 106.4, 500.4)
//! ```
//!
//! Each slot is read at most once; the retention is a per-unit attribute
//! of the produced mana, independent of any spending restriction.
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

/// The typed riders of one mana result. At least one slot is filled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendingPolicyNode {
    /// The permitted or barred uses of the mana (CR 106.6).
    pub restriction: Option<ManaSpendRestrictionNode>,
    /// `Until end of turn, you don't lose this mana as steps and phases
    /// end` (CR 106.4): the produced units survive step ends this turn.
    pub retained_until_end_of_turn: bool,
}

impl ManaSpendingPolicyNode {
    /// Whether the restriction also permits abilities of the cast objects.
    pub(in crate::oracle::canonical) fn also_activates_abilities(&self) -> bool {
        self.restriction
            .as_ref()
            .is_some_and(ManaSpendRestrictionNode::also_activates_abilities)
    }
}

/// The spending restriction slot of the riders (CR 106.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendRestrictionNode {
    /// The spells of the cast use.
    pub spells: ObjectPredicate,
    /// Every permitted use, in Oracle order; the cast use included.
    pub uses: Vec<ManaSpendUse>,
    pub spell_cant_be_countered: bool,
    /// `This mana can't be spent to cast SPELLS` (CR 106.6): the spells are
    /// the uses the mana is barred from, not the only permitted ones.
    pub prohibited: bool,
}

impl ManaSpendRestrictionNode {
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

/// Whether the retention rider starts here: `Until end of turn, you
/// don't|do not lose this mana`.
fn mana_retention_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "until")
        && word_at(cursor, 1, "end")
        && word_at(cursor, 2, "of")
        && word_at(cursor, 3, "turn")
        && cursor.peek_ahead(4) == Some(",")
        && word_at(cursor, 5, "you")
        && (word_at(cursor, 6, "don't") || (word_at(cursor, 6, "do") && word_at(cursor, 7, "not")))
}

/// `Until end of turn, you don't lose this mana as steps and phases end`.
fn recognize_mana_retention(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("mana.rider.retention.duration");
    cursor.consume_phrase(&["until", "end", "of", "turn", ",", "you"])?;
    cursor.enter("mana.rider.retention.verb");
    if word_at(cursor, 0, "do") {
        cursor.consume_phrase(&["do", "not"])?;
    } else {
        cursor.consume("don't")?;
    }
    cursor.consume("lose")?;
    cursor.enter("mana.rider.retention.object");
    cursor.consume_phrase(&["this", "mana"])?;
    cursor.enter("mana.rider.retention.boundary");
    cursor.consume_phrase(&["as", "steps", "and", "phases", "end"])
}

/// Whether a rider sentence about the produced mana starts here.
pub(in crate::oracle::canonical) fn mana_result_rider_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    mana_spending_policy_starts(cursor) || mana_retention_starts(cursor)
}

/// The riders following a production: each rider sentence after the
/// production's `.`, each slot at most once. The final terminator stays
/// with the parent. A production without riders is returned unchanged.
pub(in crate::oracle::canonical) fn recognize_mana_result_riders(
    cursor: &mut OracleTokenCursor<'_>,
    production: AddManaEffectNode,
) -> MachineResult<AddManaEffectNode> {
    let mut policy = ManaSpendingPolicyNode {
        restriction: None,
        retained_until_end_of_turn: false,
    };
    loop {
        let mut trial = cursor.clone();
        if trial.peek() == Some(".") {
            trial.consume(".")?;
        }
        if !mana_result_rider_starts(&trial) {
            break;
        }
        if mana_retention_starts(&trial) {
            trial.enter("mana.rider.retention");
            trial.require(
                !policy.retained_until_end_of_turn,
                "one retention rider for the produced mana",
            )?;
            recognize_mana_retention(&mut trial)?;
            policy.retained_until_end_of_turn = true;
        } else {
            trial.enter("mana.rider.restriction");
            trial.require(
                policy.restriction.is_none(),
                "one spending restriction for the produced mana",
            )?;
            policy.restriction = Some(recognize_mana_spending_policy(&mut trial)?);
        }
        *cursor = trial;
    }
    if policy.restriction.is_none() && !policy.retained_until_end_of_turn {
        return Ok(production);
    }
    Ok(AddManaEffectNode::Qualified {
        production: Box::new(production),
        policy,
    })
}

pub(in crate::oracle::canonical) fn recognize_mana_spending_policy(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpendRestrictionNode> {
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
    Ok(ManaSpendRestrictionNode {
        spells,
        uses,
        spell_cant_be_countered,
        prohibited,
    })
}

#[cfg(test)]
#[path = "mana_spending_policy_tests.rs"]
mod tests;
