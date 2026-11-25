//! `OBJECTS don't untap during their controller's next untap step` (CR 502.3):
//! the negated `untap` clause of a permanent subject. The subject is the
//! shared permanent operand; this child reads the negation, the lexicon verb,
//! the untap step's owner and its window.
//!
//! ```text
//! Restriction := ("doesn't" | "don't") "untap" "during" Owner Window
//! Owner       := "its controller's" | "their controllers'" | "their"
//!              | "your" | "that player's"
//! Window      := "next" "untap" ("step" | "steps")
//!              | "untap" ("step" | "steps") SourceStateDuration
//! ```
//!
//! `your` and `that player's` name one player: the restriction applies during
//! that player's next untap step only (CR 502.3, only the active player's
//! permanents untap). A possessive pronoun of the permanents names each
//! permanent's own controller. A window without `next` lasts for a
//! `for as long as` state of the source (CR 611.2b).
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, PermanentOperandNode, SourceStateDuration, VerbAgreement, VerbLemma,
    consume_lemma,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UntapRestrictionDuration {
    /// `during OWNER next untap step`: one untap step of the owner only.
    NextUntapStep,
    /// `during its controller's untap step for as long as you control
    /// SOURCE` (CR 611.2b).
    WhileSourceControlled,
    /// `... for as long as SOURCE remains tapped` (CR 611.2b).
    WhileSourceTapped,
    /// `... for as long as SOURCE remains on the battlefield` (CR 611.2b).
    WhileSourceOnBattlefield,
}

/// Whose untap step the restriction names (CR 502.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UntapStepOwner {
    /// `its controller's` / `their controllers'` / `their`: each permanent's
    /// own controller.
    ObjectController,
    /// `your` / `that player's`: one player's untap step.
    Player(LocalPlayerReference),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UntapRestrictionNode {
    pub objects: PermanentOperandNode,
    pub duration: UntapRestrictionDuration,
    pub step_owner: UntapStepOwner,
}

/// The number the subject imposes on the negation, when the operand states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SubjectNumber {
    Singular,
    Plural,
    /// A bound reference (`it`, `those creatures`) carries its own number word.
    Written,
}

/// True when the negated `untap` clause starts at the cursor.
pub(in crate::oracle::canonical) fn untap_restriction_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        let raw = raw.replace('\u{2019}', "'");
        raw.eq_ignore_ascii_case("doesn't") || raw.eq_ignore_ascii_case("don't")
    }) && cursor
        .peek_ahead(1)
        .is_some_and(|raw| super::is_verb_form(raw, VerbLemma::Untap, super::VerbForm::Base))
}

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(word))
}

fn consume_word(cursor: &mut OracleTokenCursor<'_>, word: &str) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .filter(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(word))
        .map(str::to_string)
        .ok_or_else(|| cursor.fail(&format!("`{word}`")))?;
    cursor.consume(&raw)?;
    Ok(())
}

pub(in crate::oracle::canonical) fn recognize_untap_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    objects: PermanentOperandNode,
    number: SubjectNumber,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<UntapRestrictionNode> {
    cursor.enter("effect.untap-restriction.negation");
    let plural = peek_is(cursor, "don't");
    cursor.require(
        match number {
            SubjectNumber::Singular => !plural,
            SubjectNumber::Plural => plural,
            SubjectNumber::Written => true,
        },
        "the negation its subject's number requires (`doesn't` or `don't`)",
    )?;
    consume_word(cursor, if plural { "don't" } else { "doesn't" })?;
    cursor.enter("effect.untap-restriction.verb");
    consume_lemma(cursor, VerbLemma::Untap, VerbAgreement::Base)?;
    cursor.enter("effect.untap-restriction.step");
    consume_word(cursor, "during")?;
    // The negation agrees with the subject, so its number is the number the
    // possessive pronoun of the step owner must agree with.
    let step_owner = recognize_untap_step_owner(cursor, plural)?;
    if peek_is(cursor, "next") {
        consume_word(cursor, "next")?;
        consume_word(cursor, "untap")?;
        consume_step_noun(cursor, &step_owner)?;
        return Ok(UntapRestrictionNode {
            objects,
            duration: UntapRestrictionDuration::NextUntapStep,
            step_owner,
        });
    }
    // A lasting restriction excludes each permanent from its own
    // controller's untap steps while the source state holds.
    cursor.require(
        step_owner == UntapStepOwner::ObjectController,
        "the permanents' own controllers' untap steps for a lasting restriction",
    )?;
    consume_word(cursor, "untap")?;
    consume_step_noun(cursor, &step_owner)?;
    cursor.enter("effect.untap-restriction.duration");
    let duration = match super::recognize_source_state_duration(cursor, name_links, name_failure)? {
        SourceStateDuration::Controlled => UntapRestrictionDuration::WhileSourceControlled,
        SourceStateDuration::RemainsTapped => UntapRestrictionDuration::WhileSourceTapped,
        SourceStateDuration::RemainsOnBattlefield => {
            UntapRestrictionDuration::WhileSourceOnBattlefield
        }
    };
    Ok(UntapRestrictionNode {
        objects,
        duration,
        step_owner,
    })
}

/// `step`, or `steps` after a plural possessor (`their controllers' next
/// untap steps`: one step per controller).
fn consume_step_noun(
    cursor: &mut OracleTokenCursor<'_>,
    owner: &UntapStepOwner,
) -> MachineResult<()> {
    if *owner == UntapStepOwner::ObjectController && peek_is(cursor, "steps") {
        return consume_word(cursor, "steps");
    }
    consume_word(cursor, "step")
}

/// The owner of the untap step. `its` needs a singular subject and `their` a
/// plural one; `your` and `that player's` name one player.
fn recognize_untap_step_owner(
    cursor: &mut OracleTokenCursor<'_>,
    plural_subject: bool,
) -> MachineResult<UntapStepOwner> {
    cursor.enter("effect.untap-restriction.step-owner");
    if peek_is(cursor, "your") {
        consume_word(cursor, "your")?;
        return Ok(UntapStepOwner::Player(LocalPlayerReference::Controller));
    }
    if peek_is(cursor, "its") {
        cursor.require(!plural_subject, "a singular subject for `its`")?;
        consume_word(cursor, "its")?;
        consume_word(cursor, "controller's")?;
        return Ok(UntapStepOwner::ObjectController);
    }
    if peek_is(cursor, "their") {
        cursor.require(plural_subject, "a plural subject for `their`")?;
        consume_word(cursor, "their")?;
        if peek_is(cursor, "controllers'") {
            consume_word(cursor, "controllers'")?;
        } else if peek_is(cursor, "controller's") {
            consume_word(cursor, "controller's")?;
        }
        return Ok(UntapStepOwner::ObjectController);
    }
    if let Some(player) = super::try_recognize_bound_possessive_player(cursor)? {
        return Ok(UntapStepOwner::Player(LocalPlayerReference::Bound(player)));
    }
    Err(cursor.fail(
        "an untap step owner: `its controller's`, `their controllers'`, `their`, `your` or `that player's`",
    ))
}
