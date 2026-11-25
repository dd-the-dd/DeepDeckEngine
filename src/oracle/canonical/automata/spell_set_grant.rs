//! A static rule of a spell set (CR 604.1): a keyword granted to the spells
//! a player casts (CR 613.1f: `Artifact spells you cast have convoke.`,
//! `Each noncreature spell you cast has conspire.`, `Spells you cast with
//! mana value 6 or greater have cascade.`) or a prohibition on casting them
//! (CR 101.2: `Spells with the chosen name can't be cast.`).
//!
//! ```text
//! SpellSetGrant := SUBJECT PREDICATE "." [REMINDER]
//! SUBJECT       := ["each"] SPELLS
//!                | "the" "first" SPELLS "each" "turn"     -- one spell a turn
//!                | SPELLS "you control"                  -- spells on the stack
//! PREDICATE     := ("have" | "has") KEYWORD | "can't" "be" "cast"
//! SPELLS        := the shared spell-cast collection (`recognize_spell_cast_collection`)
//! KEYWORD       := a parameterless keyword (`convoke`, `split second`)
//!                | "conspire"
//!                | "replicate" MANA-COST
//!                | "affinity" "for" QUALITY               -- CR 702.41
//! ```
//!
//! `the first SPELL you cast each turn` (an ordinal of the cast
//! history) keeps the same spell query and adds the ordinal: the grant
//! applies to a spell only while no other matching spell was cast by its
//! caster this turn. `spells you control` relates the spells to their
//! controller on the stack instead of their caster.
//!
//! The subject is the shared spell query with its caster relation; the
//! keyword is a typed atom. Which keywords the engine grants to spells is the
//! lowering's decision (`effects/spell_set_grant.rs`), never this reader's.
//! A `has` agrees with `each`, a `have` with a plural set; the frame is
//! claimed once the subject and the verb are read.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    GrantedKeyword, ManaCostNode, ObjectControllerScope, ObjectKind, ObjectPredicate,
    SpellCastQueryNode, recognize_keyword_quality, recognize_mana_cost, recognize_object_set_query,
    recognize_parameterless_keyword, recognize_spell_cast_collection,
};

/// What the static rule says of its spells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellSetPredicate {
    /// `have KEYWORD`: the spells have the keyword while cast.
    Has(SpellGrantedKeyword),
    /// `can't be cast` (CR 101.2): no player casts such a spell.
    CantBeCast,
}

/// One keyword granted to a spell set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellGrantedKeyword {
    /// A keyword without parameters (`convoke`, `improvise`, `cascade`,
    /// `split second`).
    Parameterless(GrantedKeyword),
    /// CR 702.78: conspire.
    Conspire,
    /// CR 702.56: replicate with its printed cost.
    Replicate(ManaCostNode),
    /// CR 702.41a: affinity for the permanents of a quality.
    Affinity(ObjectPredicate),
}

/// How the subject relates its spells to the ability controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellSetScope {
    /// Every matching spell (the caster relation is in the spell query).
    Each,
    /// `the first SPELL you cast each turn`: the first matching spell its
    /// caster casts in a turn.
    FirstEachTurn,
    /// `SPELLS you control`: the matching spells the ability controller
    /// controls on the stack.
    Controlled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellSetGrantNode {
    pub spells: SpellCastQueryNode,
    pub scope: SpellSetScope,
    pub predicate: SpellSetPredicate,
}

pub(in crate::oracle::canonical) enum SpellSetGrantAttempt {
    NotClaimed,
    Parsed(SpellSetGrantNode),
    Failed(MachineFailure),
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Transactional: the frame is claimed only once its subject (a spell set)
/// and its verb (`have`/`has`) are read; before that the ability stays with
/// the following readers.
pub(in crate::oracle::canonical) fn try_recognize_spell_set_grant(
    cursor: &mut OracleTokenCursor<'_>,
) -> SpellSetGrantAttempt {
    let mut trial = cursor.clone();
    let mut claimed = false;
    match read_spell_set_grant(&mut trial, &mut claimed) {
        Ok(node) => {
            *cursor = trial;
            SpellSetGrantAttempt::Parsed(node)
        }
        Err(failure) if claimed => SpellSetGrantAttempt::Failed(failure),
        Err(_) => SpellSetGrantAttempt::NotClaimed,
    }
}

fn read_spell_set_grant(
    cursor: &mut OracleTokenCursor<'_>,
    claimed: &mut bool,
) -> MachineResult<SpellSetGrantNode> {
    cursor.enter("static.spell-grant.quantifier");
    let first = word_at(cursor, 0, "the") && word_at(cursor, 1, "first");
    let each = !first && word_at(cursor, 0, "each");
    if first {
        cursor.consume_phrase(&["the", "first"])?;
    } else if each {
        cursor.consume("each")?;
    }
    cursor.enter("static.spell-grant.spells");
    let (spells, controlled) = recognize_spell_subject(cursor)?;
    let scope = if first {
        // `the first SPELL you cast each turn`: the ordinal of the cast
        // history closes the subject.
        cursor.enter("static.spell-grant.ordinal-period");
        cursor.require(!controlled, "a cast spell before `each turn`")?;
        cursor.consume_phrase(&["each", "turn"])?;
        SpellSetScope::FirstEachTurn
    } else if controlled {
        SpellSetScope::Controlled
    } else {
        SpellSetScope::Each
    };
    cursor.enter("static.spell-grant.verb");
    let cant = cursor
        .peek()
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case("can't"));
    let predicate = if cant && word_at(cursor, 1, "be") && word_at(cursor, 2, "cast") {
        // CR 101.2: `can't be cast`, the passive of every caster's action.
        let negation = cursor.peek().expect("checked negation").to_string();
        cursor.consume(&negation)?;
        cursor.consume_phrase(&["be", "cast"])?;
        *claimed = true;
        SpellSetPredicate::CantBeCast
    } else {
        // `each SPELL has`, `SPELLS have`: the verb agrees with the subject.
        cursor.consume(if each || first { "has" } else { "have" })?;
        *claimed = true;
        cursor.enter("static.spell-grant.keyword");
        SpellSetPredicate::Has(recognize_spell_granted_keyword(cursor)?)
    };
    cursor.enter("static.spell-grant.terminator");
    cursor.consume(".")?;
    // CR 207.2: one trailing parenthetical that closes the ability is
    // reminder text; anything after it is not.
    if cursor.peek() == Some("(") {
        cursor.enter("static.spell-grant.reminder");
        cursor.consume("(")?;
        while cursor.peek().is_some_and(|raw| raw != ")" && raw != "(") {
            cursor.advance_to(cursor.position() + 1)?;
        }
        cursor.consume(")")?;
    }
    cursor.enter("static.spell-grant.complete");
    cursor.finish()?;
    Ok(SpellSetGrantNode {
        spells,
        scope,
        predicate,
    })
}

/// The spell subject: the shared spell-cast collection (`spells you
/// cast`), or the same spell query related to its controller (`spells you
/// control`), which is not a cast relation. Returns whether it is the
/// controlled form.
fn recognize_spell_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(SpellCastQueryNode, bool)> {
    let mut trial = cursor.clone();
    match recognize_spell_cast_collection(&mut trial) {
        Ok(spells) => {
            *cursor = trial;
            Ok((spells, false))
        }
        Err(cast_failure) => {
            let mut controlled = cursor.clone();
            let Ok(set) = recognize_object_set_query(&mut controlled) else {
                return Err(cast_failure);
            };
            if set.exclude_source {
                return Err(cast_failure);
            }
            let Ok(query) = set.query.into_object_query(&mut controlled) else {
                return Err(cast_failure);
            };
            if query.controller != Some(ObjectControllerScope::AbilityController)
                || !query.predicate.is_in_domain(ObjectKind::Spell)
            {
                return Err(cast_failure);
            }
            *cursor = controlled;
            Ok((
                SpellCastQueryNode {
                    predicate: query.predicate,
                    caster: None,
                },
                true,
            ))
        }
    }
}

fn recognize_spell_granted_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SpellGrantedKeyword> {
    if word_at(cursor, 0, "conspire") {
        cursor.consume("conspire")?;
        return Ok(SpellGrantedKeyword::Conspire);
    }
    if word_at(cursor, 0, "replicate") {
        cursor.consume("replicate")?;
        cursor.enter("static.spell-grant.replicate-cost");
        return Ok(SpellGrantedKeyword::Replicate(recognize_mana_cost(cursor)?));
    }
    if word_at(cursor, 0, "affinity") && word_at(cursor, 1, "for") {
        cursor.consume("affinity")?;
        cursor.enter("static.spell-grant.affinity-quality");
        return Ok(SpellGrantedKeyword::Affinity(recognize_keyword_quality(
            cursor,
        )?));
    }
    recognize_parameterless_keyword(cursor).map(SpellGrantedKeyword::Parameterless)
}

#[cfg(test)]
#[path = "spell_set_grant_tests.rs"]
mod tests;
