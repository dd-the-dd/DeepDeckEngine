//! `OBJECTS don't untap during their controller's next untap step` (CR 502.3):
//! the negated `untap` clause of a permanent subject. The subject is the
//! shared permanent operand; this child reads the negation, the lexicon verb
//! and the untap-step window.
use super::super::{MachineResult, OracleTokenCursor};
use super::{PermanentOperandNode, VerbAgreement, VerbLemma, consume_lemma};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UntapRestrictionDuration {
    /// `during its controller's next untap step`: the next untap step of the
    /// permanent's controller only.
    NextUntapStep,
    /// `during its controller's untap step for as long as you control
    /// SOURCE` (CR 611.2b).
    WhileSourceControlled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UntapRestrictionNode {
    pub objects: PermanentOperandNode,
    pub duration: UntapRestrictionDuration,
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
    // The step belongs to each permanent's own controller (CR 502.3).
    if plural {
        consume_word(cursor, "their")?;
        if peek_is(cursor, "controllers'") {
            consume_word(cursor, "controllers'")?;
        } else {
            consume_word(cursor, "controller's")?;
        }
    } else {
        consume_word(cursor, "its")?;
        consume_word(cursor, "controller's")?;
    }
    if peek_is(cursor, "next") {
        consume_word(cursor, "next")?;
        consume_word(cursor, "untap")?;
        // `their controllers' next untap steps`: one next untap step per
        // controller; the plural noun agrees with the plural possessor only.
        if plural && peek_is(cursor, "steps") {
            consume_word(cursor, "steps")?;
        } else {
            consume_word(cursor, "step")?;
        }
        return Ok(UntapRestrictionNode {
            objects,
            duration: UntapRestrictionDuration::NextUntapStep,
        });
    }
    cursor.consume_phrase(&["untap", "step"])?;
    cursor.enter("effect.untap-restriction.duration");
    super::recognize_source_control_duration(cursor, name_links, name_failure)?;
    Ok(UntapRestrictionNode {
        objects,
        duration: UntapRestrictionDuration::WhileSourceControlled,
    })
}
