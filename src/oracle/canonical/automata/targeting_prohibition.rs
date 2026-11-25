//! The passive targeting prohibition of a static predicate (CR 115.4,
//! 702.11b, 702.18b): `can't be the target(s) of SOURCES`, where SOURCES
//! names the spells and abilities that can't target the subject.
//!
//! ```text
//! Prohibition := ("can't" | "cannot") "be" "the" ("target" | "targets") "of"
//!                Branch (("or" | "and") Branch)*
//! Branch      := [Atom ("or" Atom)*] "spells" [Controller]
//!              | "abilities" ["from" [Atom ("or" Atom)*] "sources"] [Controller]
//! Controller  := "your opponents control" | "an opponent controls"
//!              | "you control" | "you don't control"
//! Atom        := a typed object characteristic (`white`, `nongreen`,
//!                `artifact`, `Aura`), read by the shared predicate atom
//! ```
//!
//! A spell's prenominal characteristics and an ability's `from … sources`
//! both describe the source object that would target (CR 113.7, 115.1): the
//! node keeps one source predicate per branch. A controller written after
//! the last branch is shared by earlier bare branches (`spells or abilities
//! your opponents control`); a branch with its own source clause or
//! controller keeps its own scope. The static parent owns the subject.
use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectControllerScope, ObjectPredicate, recognize_object_predicate_atom};
use crate::oracle::characteristic_vocabulary::AcceptedForms;

/// What kind of stack object a branch names (CR 113.7, 115.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetingSourceKind {
    Spell,
    /// Activated and triggered abilities alike.
    Ability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetingSourceBranch {
    pub kind: TargetingSourceKind,
    /// The characteristics of the targeting source; `None` is any source.
    pub source: Option<ObjectPredicate>,
    /// The targeting object's controller relative to the ability
    /// controller; `None` is any controller.
    pub controller: Option<ObjectControllerScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetingProhibitionNode {
    pub branches: Vec<TargetingSourceBranch>,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

/// `can't be the target(s) of` at the cursor: a lexical probe.
pub(in crate::oracle::canonical) fn targeting_prohibition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_at(cursor, 0, "can't") || word_at(cursor, 0, "cannot"))
        && word_at(cursor, 1, "be")
        && word_at(cursor, 2, "the")
        && (word_at(cursor, 3, "target") || word_at(cursor, 3, "targets"))
        && word_at(cursor, 4, "of")
}

/// One branch's own reading, before the shared controller is distributed.
struct BranchReading {
    branch: TargetingSourceBranch,
    /// A `from … sources` clause or a controller is local to this branch.
    locally_scoped: bool,
}

pub(in crate::oracle::canonical) fn recognize_targeting_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetingProhibitionNode> {
    cursor.enter("static.targeting-prohibition.head");
    let negation = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&negation)?;
    cursor.consume_phrase(&["be", "the"])?;
    let noun = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&noun)?;
    cursor.consume("of")?;
    let mut readings = Vec::new();
    loop {
        readings.push(recognize_branch(cursor)?);
        cursor.enter("static.targeting-prohibition.connector");
        if (word_at(cursor, 0, "or") || word_at(cursor, 0, "and")) && branch_starts_at(cursor, 1) {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
            continue;
        }
        break;
    }
    // A trailing controller is shared by the earlier bare branches.
    let shared = readings
        .last()
        .and_then(|reading| reading.branch.controller);
    let last = readings.len() - 1;
    let branches = readings
        .into_iter()
        .enumerate()
        .map(|(index, mut reading)| {
            if index < last && !reading.locally_scoped {
                reading.branch.controller = shared;
            }
            reading.branch
        })
        .collect();
    cursor.enter("static.targeting-prohibition.complete");
    Ok(TargetingProhibitionNode { branches })
}

/// A branch noun, possibly after its prenominal characteristics.
fn branch_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let mut ahead = cursor.clone();
    if ahead.advance_to(ahead.position() + offset).is_err() {
        return false;
    }
    loop {
        if word_at(&ahead, 0, "spells") || word_at(&ahead, 0, "abilities") {
            return true;
        }
        if recognize_object_predicate_atom(&mut ahead, AcceptedForms::SingularOnly).is_err() {
            return false;
        }
        if word_at(&ahead, 0, "or") && ahead.advance_to(ahead.position() + 1).is_err() {
            return false;
        }
    }
}

fn recognize_branch(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<BranchReading> {
    cursor.enter("static.targeting-prohibition.branch");
    let prenominal = recognize_optional_characteristics(cursor)?;
    cursor.enter("static.targeting-prohibition.kind");
    let kind = if word_at(cursor, 0, "spells") {
        cursor.consume("spells")?;
        TargetingSourceKind::Spell
    } else if word_at(cursor, 0, "abilities") {
        cursor.require(
            prenominal.is_none(),
            "an ability's source characteristics in `from … sources`",
        )?;
        cursor.consume("abilities")?;
        TargetingSourceKind::Ability
    } else {
        return Err(cursor.fail("`spells` or `abilities` that would target the object"));
    };
    let mut source = prenominal;
    let mut locally_scoped = false;
    if kind == TargetingSourceKind::Ability && word_at(cursor, 0, "from") {
        cursor.enter("static.targeting-prohibition.source");
        cursor.consume("from")?;
        source = recognize_optional_characteristics(cursor)?;
        cursor.require(
            source.is_some(),
            "the characteristics of the sources (`from white sources`)",
        )?;
        cursor.consume("sources")?;
        locally_scoped = true;
    }
    let controller = recognize_optional_controller(cursor)?;
    locally_scoped |= controller.is_some();
    Ok(BranchReading {
        branch: TargetingSourceBranch {
            kind,
            source,
            controller,
        },
        locally_scoped,
    })
}

/// `ATOM ("or" ATOM)*` before a noun: one characteristic, or the union of
/// the coordinated ones (`blue or black spells`).
fn recognize_optional_characteristics(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectPredicate>> {
    let mut atoms = Vec::new();
    loop {
        if word_at(cursor, 0, "spells")
            || word_at(cursor, 0, "abilities")
            || word_at(cursor, 0, "sources")
        {
            break;
        }
        cursor.enter("static.targeting-prohibition.characteristic");
        atoms.push(recognize_object_predicate_atom(
            cursor,
            AcceptedForms::SingularOnly,
        )?);
        if word_at(cursor, 0, "or")
            && !(word_at(cursor, 1, "spells") || word_at(cursor, 1, "abilities"))
        {
            cursor.consume("or")?;
            continue;
        }
    }
    Ok(match atoms.len() {
        0 => None,
        1 => atoms.pop(),
        _ => Some(ObjectPredicate::Union(atoms)),
    })
}

/// The controller of the targeting spell or ability, relative to the
/// ability controller.
fn recognize_optional_controller(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectControllerScope>> {
    if word_at(cursor, 0, "your") && word_at(cursor, 1, "opponents") {
        cursor.enter("static.targeting-prohibition.controller");
        cursor.consume_phrase(&["your", "opponents", "control"])?;
        return Ok(Some(ObjectControllerScope::OpponentOfAbilityController));
    }
    if word_at(cursor, 0, "an") && word_at(cursor, 1, "opponent") {
        cursor.enter("static.targeting-prohibition.controller");
        cursor.consume_phrase(&["an", "opponent", "controls"])?;
        return Ok(Some(ObjectControllerScope::OpponentOfAbilityController));
    }
    if word_at(cursor, 0, "you") && (word_at(cursor, 1, "control") || word_at(cursor, 1, "don't")) {
        cursor.enter("static.targeting-prohibition.controller");
        cursor.consume("you")?;
        if word_at(cursor, 0, "don't") {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
            cursor.consume("control")?;
            return Ok(Some(ObjectControllerScope::NotAbilityController));
        }
        cursor.consume("control")?;
        return Ok(Some(ObjectControllerScope::AbilityController));
    }
    Ok(None)
}
