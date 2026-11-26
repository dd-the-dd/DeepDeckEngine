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
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventMultiplicity, ObjectKind, ObjectQueryNode, QuantifiedEventSubjectNode, VerbAgreement,
    VerbLemma, consume_lemma, lemma_at, recognize_object_query, recognize_quantified_event_subject,
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

#[cfg(test)]
#[path = "object_action_event_tests.rs"]
mod tests;
