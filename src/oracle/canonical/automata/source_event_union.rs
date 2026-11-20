//! Events whose subject is the ability source itself (CR 603.2, 603.6c,
//! 603.10a), as an ordered union of independent event verbs:
//!
//! ```text
//! SourceEvent := SOURCE SourceVerb ("or" SourceVerb)*
//! SourceVerb  := "enters" ["the battlefield"] | "dies" | "attacks" | "blocks"
//!              | "leaves the battlefield"
//!              | "is put into a graveyard from the battlefield"
//!              | "becomes" ("tapped" | "untapped" | "blocked")
//! ```
//!
//! The subject is the shared source reference; each verb is one event atom.
//! A union selects one event occurrence (`attacks or blocks`); it never
//! multiplies the subject. Runtime support is decided by the lowering, which
//! fails explicitly for verbs without an inspected matcher.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, recognize_source_reference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceEventVerb {
    Entered,
    Died,
    Attacked,
    Blocked,
    LeftBattlefield,
    /// `is put into a graveyard from the battlefield` (any permanent type).
    PutIntoGraveyardFromBattlefield,
    BecameTapped,
    BecameUntapped,
    BecameBlocked,
}

impl SourceEventVerb {
    /// Departure verbs observe the source after it left the battlefield
    /// (look-back triggers, CR 603.10a).
    pub(in crate::oracle::canonical) fn is_departure(self) -> bool {
        matches!(
            self,
            Self::Died | Self::LeftBattlefield | Self::PutIntoGraveyardFromBattlefield
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEventUnionNode {
    pub source: LocalObjectReference,
    /// Written order; every verb is distinct.
    pub verbs: Vec<SourceEventVerb>,
}

fn peek_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn recognize_source_event_verb(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SourceEventVerb> {
    cursor.enter("event.source.verb");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a source event verb"))?;
    let verb = match head.as_str() {
        "enters" => {
            cursor.consume("enters")?;
            if peek_is(cursor, 0, "the") && peek_is(cursor, 1, "battlefield") {
                cursor.consume_phrase(&["the", "battlefield"])?;
            }
            SourceEventVerb::Entered
        }
        "dies" => {
            cursor.consume("dies")?;
            SourceEventVerb::Died
        }
        "attacks" => {
            cursor.consume("attacks")?;
            SourceEventVerb::Attacked
        }
        "blocks" => {
            cursor.consume("blocks")?;
            SourceEventVerb::Blocked
        }
        "leaves" => {
            cursor.enter("event.source.leaves");
            cursor.consume_phrase(&["leaves", "the", "battlefield"])?;
            SourceEventVerb::LeftBattlefield
        }
        "is" => {
            cursor.enter("event.source.put-into-graveyard");
            cursor.consume_phrase(&["is", "put", "into"])?;
            cursor.enter("event.source.put-into-graveyard.zone");
            // `your graveyard` adds an owner relation without a matcher.
            cursor.consume_phrase(&["a", "graveyard"])?;
            cursor.enter("event.source.put-into-graveyard.origin");
            cursor.consume_phrase(&["from", "the", "battlefield"])?;
            SourceEventVerb::PutIntoGraveyardFromBattlefield
        }
        "becomes" => {
            cursor.consume("becomes")?;
            cursor.enter("event.source.becomes");
            let state = cursor
                .peek()
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| cursor.fail("`tapped`, `untapped` or `blocked`"))?;
            let verb = match state.as_str() {
                "tapped" => SourceEventVerb::BecameTapped,
                "untapped" => SourceEventVerb::BecameUntapped,
                "blocked" => SourceEventVerb::BecameBlocked,
                _ => return Err(cursor.fail("`tapped`, `untapped` or `blocked`")),
            };
            cursor.consume(&state)?;
            verb
        }
        _ => {
            return Err(cursor.fail(
                "enters, dies, attacks, blocks, leaves the battlefield, is put into a graveyard, or becomes STATE",
            ));
        }
    };
    Ok(verb)
}

/// Reads `SOURCE VERB [or VERB]...` after the trigger word. The enclosing
/// envelope owns the trigger word, the comma and the body.
pub(in crate::oracle::canonical) fn recognize_source_event_union(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEventUnionNode> {
    cursor.enter("event.source.subject");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    let mut verbs = vec![recognize_source_event_verb(cursor)?];
    while peek_is(cursor, 0, "or") {
        cursor.enter("event.source.union");
        cursor.consume("or")?;
        let verb = recognize_source_event_verb(cursor)?;
        cursor.enter("event.source.union-distinct");
        cursor.require(!verbs.contains(&verb), "a distinct event in the union")?;
        verbs.push(verb);
    }
    cursor.enter("event.source.complete");
    Ok(SourceEventUnionNode { source, verbs })
}

/// Reads `you cycle SOURCE` (CR 702.29): the controller cycles the ability
/// source itself. Other cycled objects or actors have their own subjects.
pub(in crate::oracle::canonical) fn recognize_source_cycled_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<super::LocalTriggerEventNode> {
    cursor.enter("event.source-cycled.actor");
    cursor.consume("you")?;
    cursor.enter("event.source-cycled.action");
    cursor.consume("cycle")?;
    cursor.enter("event.source-cycled.object");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.source-cycled.complete");
    Ok(super::LocalTriggerEventNode::SourceCycled { source })
}

#[cfg(test)]
#[path = "source_event_union_tests.rs"]
mod tests;
