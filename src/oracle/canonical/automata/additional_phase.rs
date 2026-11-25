//! Phases added to the current turn (CR 500.8), one typed action:
//!
//! ```text
//! AddedPhases := Anchor [","] Existential
//!              | Existential Anchor
//! Anchor      := "after" "this" ["main" | "combat"] "phase"
//!              | "after" "this" "one"                 -- this phase
//! Existential := "there" ("is" | "'s") "an" "additional" "combat" "phase"
//!                [[","] "followed" "by" "an" "additional" "main" "phase"]
//! ```
//!
//! The anchor names the phase the added phases follow; the added main phase
//! is an independent attribute. Conditions (`if it's your main phase`) and
//! the actions written before the clause belong to the enclosing program.
use super::super::{MachineResult, OracleTokenCursor};

/// The phase the added phases follow, resolved when the action resolves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AddedPhaseAnchor {
    /// `after this phase` / `after this one`.
    CurrentPhase,
    /// `after this main phase`.
    CurrentMainPhase,
    /// `after this combat phase`.
    CurrentCombatPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AdditionalPhasesNode {
    pub anchor: AddedPhaseAnchor,
    /// `followed by an additional main phase` (CR 505.1a).
    pub followed_by_main_phase: bool,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `there is` / `there's` at `offset`: the existential head.
fn existential_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if word_at(cursor, offset, "there") && word_at(cursor, offset + 1, "is") {
        Some(2)
    } else if word_at(cursor, offset, "there's") || word_at(cursor, offset, "there\u{2019}s") {
        Some(1)
    } else {
        None
    }
}

/// Whether an added-phase action starts at the cursor: the phase anchor
/// before the existential, or the existential of an additional combat
/// phase. A lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn additional_phases_start(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    // `after this [main|combat] phase [,] there ...`
    let anchor = word_at(cursor, 0, "after")
        && word_at(cursor, 1, "this")
        && (3..=5).any(|offset| existential_width(cursor, offset).is_some());
    let existential = existential_width(cursor, 0).is_some_and(|width| {
        word_at(cursor, width, "an")
            && word_at(cursor, width + 1, "additional")
            && word_at(cursor, width + 2, "combat")
    });
    anchor || existential
}

fn recognize_anchor(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<AddedPhaseAnchor> {
    cursor.enter("effect.additional-phase.anchor");
    cursor.consume_phrase(&["after", "this"])?;
    let anchor = if word_at(cursor, 0, "one") {
        cursor.consume("one")?;
        return Ok(AddedPhaseAnchor::CurrentPhase);
    } else if word_at(cursor, 0, "main") {
        cursor.consume("main")?;
        AddedPhaseAnchor::CurrentMainPhase
    } else if word_at(cursor, 0, "combat") {
        cursor.consume("combat")?;
        AddedPhaseAnchor::CurrentCombatPhase
    } else {
        AddedPhaseAnchor::CurrentPhase
    };
    cursor.consume("phase")?;
    Ok(anchor)
}

/// `there is an additional combat phase [followed by an additional main
/// phase]`; returns whether a main phase follows.
fn recognize_existential(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    cursor.enter("effect.additional-phase.existential");
    let width = existential_width(cursor, 0)
        .ok_or_else(|| cursor.fail("`there is` before the added phase"))?;
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("effect.additional-phase.combat");
    cursor.consume_phrase(&["an", "additional", "combat", "phase"])?;
    let comma = usize::from(cursor.peek() == Some(","));
    if !word_at(cursor, comma, "followed") {
        return Ok(false);
    }
    cursor.enter("effect.additional-phase.main");
    if comma == 1 {
        cursor.consume(",")?;
    }
    cursor.consume_phrase(&["followed", "by", "an", "additional", "main", "phase"])?;
    Ok(true)
}

/// Reads one added-phase action (CR 500.8). The enclosing sequence owns the
/// sentence terminator and any condition.
pub(in crate::oracle::canonical) fn recognize_additional_phases(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AdditionalPhasesNode> {
    cursor.enter("effect.additional-phase");
    if word_at(cursor, 0, "after") {
        let anchor = recognize_anchor(cursor)?;
        if cursor.peek() == Some(",") {
            cursor.consume(",")?;
        }
        let followed_by_main_phase = recognize_existential(cursor)?;
        return Ok(AdditionalPhasesNode {
            anchor,
            followed_by_main_phase,
        });
    }
    let mut followed_by_main_phase = recognize_existential(cursor)?;
    cursor.enter("effect.additional-phase.postposed-anchor");
    let anchor = recognize_anchor(cursor)?;
    // `there is an additional combat phase after this phase, followed by an
    // additional main phase`: the main phase follows the postposed anchor.
    if !followed_by_main_phase {
        let comma = usize::from(cursor.peek() == Some(","));
        if word_at(cursor, comma, "followed") {
            cursor.enter("effect.additional-phase.main");
            if comma == 1 {
                cursor.consume(",")?;
            }
            cursor.consume_phrase(&["followed", "by", "an", "additional", "main", "phase"])?;
            followed_by_main_phase = true;
        }
    }
    Ok(AdditionalPhasesNode {
        anchor,
        followed_by_main_phase,
    })
}

#[cfg(test)]
#[path = "additional_phase_tests.rs"]
mod tests;
