//! Object events whose subject performs a keyword action on another object
//! (CR 702.110b): `a creature you control exploits a nontoken creature`.
//!
//! ```text
//! ObjectActionEvent := ("a" | "an" | "another") SUBJECT-QUERY
//!                      VERB ("a" | "an") COUNTERPART-QUERY
//! VERB              := a keyword-action lemma whose subject is the acting
//!                      permanent and whose object is a second permanent:
//!                      `exploits` (CR 702.110b)
//! ```
//!
//! The subject is the shared quantified event subject (one occurrence per
//! acting permanent); the counterpart is the shared object query. The ability
//! source as subject (`this creature exploits a creature`) is a source event
//! verb (`source_event_union.rs`). The enclosing event clause owns the trigger
//! word and the boundary.
//!
//! The passive of a zone-moving lemma is the same subject's departure from
//! the battlefield to a written zone (CR 603.6c, 603.10a):
//!
//! ```text
//! ObjectMoveEvent := SUBJECT ("is" | "are") MOVE-PARTICIPLE
//! MOVE-PARTICIPLE := "exiled" "from" "the" "battlefield"
//!                  | "returned" "to" [OWNER] "hand" ["from" "the" "battlefield"]
//! OWNER           := "its" "owner's" | "their" "owners'"
//! ```
//!
//! The participle is a lexicon lemma (`exile`, `return`); the auxiliary
//! agrees with the subject's number. A hand of a written player (`your
//! hand`, `a player's hand`) is a relation to the card's owner that the
//! departure record does not evaluate, and fails at its own state.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventMultiplicity, ObjectKind, ObjectQueryNode, QuantifiedEventSubjectNode, VerbAgreement,
    VerbForm, VerbLemma, consume_lemma, is_verb_form, lemma_at, quantified_plural_subject_starts,
    recognize_object_query, recognize_quantified_event_subject,
};

/// The keyword actions one permanent performs on another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectActionVerb {
    /// `exploits PERMANENT` (CR 702.110b): the exploit ability of the subject
    /// sacrificed the counterpart as it resolved.
    Exploit,
}

impl ObjectActionVerb {
    fn lemma(self) -> VerbLemma {
        match self {
            Self::Exploit => VerbLemma::Exploit,
        }
    }

    /// The event-pattern lemma the runtime publishes for this action.
    pub(in crate::oracle::canonical) fn pattern_lemma(self) -> &'static str {
        match self {
            Self::Exploit => "exploit",
        }
    }
}

const OBJECT_ACTION_VERBS: [ObjectActionVerb; 1] = [ObjectActionVerb::Exploit];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectActionEventNode {
    pub subject: QuantifiedEventSubjectNode,
    pub verb: ObjectActionVerb,
    pub counterpart: ObjectQueryNode,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Reads `a|an|another QUERY VERB a|an QUERY` after the trigger word. A
/// subject that is not followed by an object-action verb is a mismatch the
/// enclosing clause rolls back; once the verb is read, an unsupported
/// counterpart fails at its own state.
pub(in crate::oracle::canonical) fn recognize_object_action_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectActionEventNode> {
    cursor.enter("event.object-action.subject");
    cursor.require(
        word_at(cursor, 0, "a") || word_at(cursor, 0, "an") || word_at(cursor, 0, "another"),
        "`a`, `an` or `another` before the acting permanent",
    )?;
    let subject = recognize_quantified_event_subject(cursor)?;
    cursor.enter("event.object-action.subject-domain");
    cursor.require(
        subject.multiplicity == EventMultiplicity::PerObject
            && subject
                .query
                .predicate
                .supports_domain(ObjectKind::Permanent)
            && subject.query.predicate.has_nominal_head(),
        "one acting permanent with a nominal head",
    )?;
    cursor.enter("event.object-action.verb");
    let verb = OBJECT_ACTION_VERBS
        .into_iter()
        .find(|verb| lemma_at(cursor, verb.lemma(), VerbAgreement::ThirdPersonSingular))
        .ok_or_else(|| cursor.fail("an object-action verb (`exploits`)"))?;
    consume_lemma(cursor, verb.lemma(), VerbAgreement::ThirdPersonSingular)?;
    cursor.enter("event.object-action.counterpart");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an")
        .ok_or_else(|| cursor.fail("`a` or `an` before the acted-on permanent"))?;
    cursor.consume(&article)?;
    let counterpart = recognize_object_query(cursor)?;
    cursor.enter("event.object-action.counterpart-domain");
    cursor.require(
        counterpart.predicate.supports_domain(ObjectKind::Permanent)
            && counterpart.predicate.has_nominal_head()
            && !counterpart.exclude_source,
        "an acted-on permanent with a nominal head",
    )?;
    Ok(ObjectActionEventNode {
        subject,
        verb,
        counterpart,
    })
}

/// The zone a moved permanent goes to from the battlefield.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectMoveDestination {
    /// `is exiled from the battlefield` (CR 406.3).
    Exile,
    /// `is returned to its owner's hand` (CR 400.3: the owner's hand).
    Hand,
}

impl ObjectMoveDestination {
    /// The runtime zone name of the move record.
    pub(in crate::oracle::canonical) fn zone(self) -> &'static str {
        match self {
            Self::Exile => "exile",
            Self::Hand => "hand",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectMoveEventNode {
    pub subject: QuantifiedEventSubjectNode,
    pub destination: ObjectMoveDestination,
}

/// The participle of a zone-moving lemma after the auxiliary of `agreement`
/// at the cursor (a lexical probe that commits nothing).
fn move_participle_at(cursor: &OracleTokenCursor<'_>, auxiliary: &str) -> Option<VerbLemma> {
    if !word_at(cursor, 0, auxiliary) {
        return None;
    }
    let participle = cursor.peek_ahead(1)?;
    [VerbLemma::Exile, VerbLemma::Return]
        .into_iter()
        .find(|lemma| is_verb_form(participle, *lemma, VerbForm::PastParticiple))
}

/// Reads `SUBJECT is|are exiled from the battlefield` or `SUBJECT is|are
/// returned to its owner's hand [from the battlefield]` after the trigger
/// word. A subject without a moving participle after it is a mismatch the
/// enclosing clause rolls back; once the participle is read, an unsupported
/// complement fails at its own state.
pub(in crate::oracle::canonical) fn recognize_object_move_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectMoveEventNode> {
    cursor.enter("event.object-move.subject");
    let plural = quantified_plural_subject_starts(cursor);
    cursor.require(
        plural
            || word_at(cursor, 0, "a")
            || word_at(cursor, 0, "an")
            || word_at(cursor, 0, "another"),
        "`a`, `an`, `another` or `N or more` before the moved permanent",
    )?;
    let subject = recognize_quantified_event_subject(cursor)?;
    cursor.enter("event.object-move.subject-domain");
    cursor.require(
        subject
            .query
            .predicate
            .supports_domain(ObjectKind::Permanent)
            && subject.query.predicate.has_nominal_head(),
        "a moved permanent with a nominal head",
    )?;
    let auxiliary = if subject.multiplicity == EventMultiplicity::PerObject {
        "is"
    } else {
        "are"
    };
    cursor.enter("event.object-move.participle");
    let lemma = move_participle_at(cursor, auxiliary)
        .ok_or_else(|| cursor.fail("`is exiled` or `is returned` agreeing with the subject"))?;
    cursor.consume(auxiliary)?;
    let participle = cursor.peek().map(str::to_string).unwrap_or_default();
    cursor.consume(&participle)?;
    let destination = match lemma {
        VerbLemma::Exile => {
            cursor.enter("event.object-move.exile-origin");
            cursor.consume_phrase(&["from", "the", "battlefield"])?;
            ObjectMoveDestination::Exile
        }
        _ => {
            cursor.enter("event.object-move.hand");
            cursor.consume("to")?;
            if word_at(cursor, 0, "its") && word_at(cursor, 1, "owner's") {
                cursor.consume_phrase(&["its", "owner's"])?;
            } else if word_at(cursor, 0, "their") && word_at(cursor, 1, "owners'") {
                cursor.consume_phrase(&["their", "owners'"])?;
            }
            cursor.enter("event.object-move.hand-owner");
            cursor.require(
                word_at(cursor, 0, "hand"),
                "its owner's hand (a written hand owner is a relation the departure record does not evaluate)",
            )?;
            cursor.consume("hand")?;
            if word_at(cursor, 0, "from") {
                cursor.enter("event.object-move.hand-origin");
                cursor.consume_phrase(&["from", "the", "battlefield"])?;
            }
            ObjectMoveDestination::Hand
        }
    };
    Ok(ObjectMoveEventNode {
        subject,
        destination,
    })
}

/// A failure of the complement after the moving participle was read: the
/// move frame owns the clause from there.
pub(in crate::oracle::canonical) fn object_move_complement_failure(
    failure: &super::super::MachineFailure,
) -> bool {
    matches!(
        failure.state,
        "event.object-move.exile-origin"
            | "event.object-move.hand"
            | "event.object-move.hand-owner"
            | "event.object-move.hand-origin"
    )
}

#[cfg(test)]
#[path = "object_action_event_tests.rs"]
mod tests;
