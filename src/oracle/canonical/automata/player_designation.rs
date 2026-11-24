//! Player designations (CR 724 the monarch, CR 725 the initiative) and the
//! Ring's temptation (CR 701.54), read as ordinary clauses of the shared
//! verb lexicon.
//!
//! ```text
//! Designation := PLAYER (become|becomes) "the" DESIGNATION   -- `monarch`
//!              | PLAYER (take|takes) "the" DESIGNATION       -- `initiative`
//! Temptation  := "the" "Ring" "tempts" PLAYER
//! ```
//!
//! The registry pairs each designation with the lemma that gives it; the
//! acting player is the clause's player subject (imperative, `you`, a
//! performer). Permanent designations (`monstrous`, `renowned`) are a
//! separate registry (`designation.rs`): a player never takes them.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    PlayerEffectSubject, VerbAgreement, VerbForm, VerbLemma, consume_lemma, lookup_verb, verb_entry,
};

/// A designation a player has (CR 724.1, 725.1): at most one player has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum AcquiredDesignation {
    /// CR 724: the monarch draws at the beginning of their end step.
    Monarch,
    /// CR 725: the initiative ventures into Undercity.
    Initiative,
}

/// One registry row: the noun after `the` and the lemma that gives it.
struct DesignationEntry {
    designation: AcquiredDesignation,
    noun: &'static str,
    lemma: VerbLemma,
}

const PLAYER_DESIGNATIONS: &[DesignationEntry] = &[
    DesignationEntry {
        designation: AcquiredDesignation::Monarch,
        noun: "monarch",
        lemma: VerbLemma::Become,
    },
    DesignationEntry {
        designation: AcquiredDesignation::Initiative,
        noun: "initiative",
        lemma: VerbLemma::Take,
    },
];

/// An action whose effect is a player designation or the Ring's temptation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum PlayerDesignationAction {
    /// `PLAYER becomes the monarch` / `takes the initiative`: the acting
    /// player gets the designation; whoever had it loses it (CR 724.4, 725.4).
    Acquire(AcquiredDesignation),
    /// `the Ring tempts PLAYER` (CR 701.54a): the player chooses a creature
    /// they control as their Ring-bearer and the Ring's level rises.
    RingTemptation,
}

/// The designation the clause at the cursor gives its acting player, in any
/// agreement: the registry lemma followed by `the NOUN`. `None` makes no claim.
pub(in crate::oracle::canonical) fn player_designation_follows(
    cursor: &OracleTokenCursor<'_>,
) -> Option<AcquiredDesignation> {
    let verb = cursor.peek()?;
    PLAYER_DESIGNATIONS
        .iter()
        .find(|entry| {
            lookup_verb(verb, std::slice::from_ref(&entry.lemma)).is_some_and(|(_, forms)| {
                forms.contains(&VerbForm::Base) || forms.contains(&VerbForm::ThirdPersonSingular)
            }) && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
                && cursor
                    .peek_ahead(2)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(entry.noun))
        })
        .map(|entry| entry.designation)
}

/// Subject-free `become(s) the monarch` / `take(s) the initiative`; the
/// caller resolves the acting player and the verb agreement.
pub(in crate::oracle::canonical) fn recognize_player_designation_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<AcquiredDesignation> {
    cursor.enter("player-designation.verb");
    let designation = player_designation_follows(cursor).ok_or_else(|| {
        cursor.fail("a player designation (`become the monarch`, `take the initiative`)")
    })?;
    let entry = PLAYER_DESIGNATIONS
        .iter()
        .find(|entry| entry.designation == designation)
        .expect("a registry row for the claimed designation");
    consume_lemma(cursor, entry.lemma, form)?;
    cursor.enter("player-designation.noun");
    cursor.consume_phrase(&["the", entry.noun])?;
    Ok(designation)
}

/// True when `the Ring tempts` starts here (CR 701.54a).
pub(in crate::oracle::canonical) fn ring_temptation_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("ring"))
        && cursor.peek_ahead(2).is_some_and(|raw| {
            lookup_verb(raw, &[VerbLemma::Tempt])
                .is_some_and(|(_, forms)| forms.contains(&VerbForm::ThirdPersonSingular))
        })
}

/// `the Ring tempts PLAYER`: the Ring is the subject (third person
/// singular); the tempted player is the object. Only the controller (`you`)
/// is a tempted player of the effect language; another object fails here.
pub(in crate::oracle::canonical) fn recognize_ring_temptation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerEffectSubject> {
    cursor.enter("player-designation.ring.subject");
    cursor.consume_phrase(&["the", "ring"])?;
    cursor.enter("player-designation.ring.verb");
    cursor.consume(verb_entry(VerbLemma::Tempt).surface(VerbForm::ThirdPersonSingular))?;
    cursor.enter("player-designation.ring.object");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you")),
        "the tempted player `you` (CR 701.54a)",
    )?;
    cursor.consume("you")?;
    Ok(PlayerEffectSubject::Controller)
}

#[cfg(test)]
#[path = "player_designation_tests.rs"]
mod tests;
