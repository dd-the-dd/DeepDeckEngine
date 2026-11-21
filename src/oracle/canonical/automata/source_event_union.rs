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
use super::{
    LocalObjectReference, VerbAgreement, VerbLemma, consume_lemma, lemma_at,
    recognize_source_reference,
};

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

/// The other combatant of a block event: the attacker a source `blocks`, or
/// the blocker it `becomes blocked by`. A counterpart makes the pattern match
/// once per block pair (CR 509.3b, 509.3d); `one or more` matches once each
/// combat, at the first matching counterpart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventCounterpartNode {
    pub verb: SourceEventVerb,
    pub query: super::ObjectQueryNode,
    pub one_or_more: bool,
}

/// A generic modifier of an event pattern. Each is a typed attribute that a
/// runtime matcher must honor; a pattern carrying one the engine has no
/// matcher for fails in lowering as an engine gap, never widened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceEventModifier {
    /// `attacks alone` (CR 506.5).
    AttackingAlone,
    /// `blocks QUERY` / `becomes blocked by QUERY`.
    Counterpart(EventCounterpartNode),
    /// `and isn't blocked` (CR 509.1h, the declare-blockers step).
    NotBlocked,
    /// `for the first time each turn`.
    FirstTimeEachTurn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEventUnionNode {
    pub source: LocalObjectReference,
    /// Written order; every verb is distinct.
    pub verbs: Vec<SourceEventVerb>,
    /// Written order; empty for an unmodified pattern.
    pub modifiers: Vec<SourceEventModifier>,
}

fn peek_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn verb_at(cursor: &OracleTokenCursor<'_>, lemma: VerbLemma) -> bool {
    lemma_at(cursor, lemma, VerbAgreement::ThirdPersonSingular)
}

fn consume_verb(cursor: &mut OracleTokenCursor<'_>, lemma: VerbLemma) -> MachineResult<()> {
    consume_lemma(cursor, lemma, VerbAgreement::ThirdPersonSingular)
}

/// Reads the counterpart object of `blocks` / `becomes blocked by` with the
/// shared object query. Even `a creature` is kept: it changes the event's
/// multiplicity from once per combat to once per block pair (CR 509.3d).
fn read_counterpart(
    cursor: &mut OracleTokenCursor<'_>,
    verb: SourceEventVerb,
    modifiers: &mut Vec<SourceEventModifier>,
) -> MachineResult<()> {
    cursor.enter("event.source.counterpart");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| matches!(raw.as_str(), "a" | "an" | "one"))
        .ok_or_else(|| cursor.fail("`a creature` or a creature counterpart"))?;
    let one_or_more = article == "one";
    if one_or_more {
        cursor.consume_phrase(&["one", "or", "more"])?;
    } else {
        cursor.consume(&article)?;
    }
    let query = super::recognize_object_query_before_separator(cursor)?;
    modifiers.push(SourceEventModifier::Counterpart(EventCounterpartNode {
        verb,
        query,
        one_or_more,
    }));
    Ok(())
}

fn recognize_source_event_verb(
    cursor: &mut OracleTokenCursor<'_>,
    modifiers: &mut Vec<SourceEventModifier>,
) -> MachineResult<SourceEventVerb> {
    cursor.enter("event.source.verb");
    // Each verb is a lexicon lemma in the subject's agreement (the shared
    // clause); the frame after it is read here as event operands.
    if verb_at(cursor, VerbLemma::Enter) {
        consume_verb(cursor, VerbLemma::Enter)?;
        if peek_is(cursor, 0, "the") && peek_is(cursor, 1, "battlefield") {
            cursor.consume_phrase(&["the", "battlefield"])?;
        }
        return Ok(SourceEventVerb::Entered);
    }
    if verb_at(cursor, VerbLemma::Die) {
        consume_verb(cursor, VerbLemma::Die)?;
        return Ok(SourceEventVerb::Died);
    }
    if verb_at(cursor, VerbLemma::Attack) {
        consume_verb(cursor, VerbLemma::Attack)?;
        if peek_is(cursor, 0, "alone") {
            cursor.consume("alone")?;
            modifiers.push(SourceEventModifier::AttackingAlone);
        }
        return Ok(SourceEventVerb::Attacked);
    }
    if verb_at(cursor, VerbLemma::Block) {
        consume_verb(cursor, VerbLemma::Block)?;
        if peek_is(cursor, 0, "a") || peek_is(cursor, 0, "an") || peek_is(cursor, 0, "one") {
            read_counterpart(cursor, SourceEventVerb::Blocked, modifiers)?;
        }
        return Ok(SourceEventVerb::Blocked);
    }
    if verb_at(cursor, VerbLemma::Leave) {
        cursor.enter("event.source.leaves");
        consume_verb(cursor, VerbLemma::Leave)?;
        cursor.consume_phrase(&["the", "battlefield"])?;
        return Ok(SourceEventVerb::LeftBattlefield);
    }
    if peek_is(cursor, 0, "is") && peek_is(cursor, 1, "put") {
        // The passive of `put` (`is put into a graveyard from the battlefield`).
        cursor.enter("event.source.put-into-graveyard");
        cursor.consume("is")?;
        consume_lemma(cursor, VerbLemma::Put, VerbAgreement::Base)?;
        cursor.consume("into")?;
        cursor.enter("event.source.put-into-graveyard.zone");
        // `your graveyard` adds an owner relation without a matcher.
        cursor.consume_phrase(&["a", "graveyard"])?;
        cursor.enter("event.source.put-into-graveyard.origin");
        cursor.consume_phrase(&["from", "the", "battlefield"])?;
        return Ok(SourceEventVerb::PutIntoGraveyardFromBattlefield);
    }
    if verb_at(cursor, VerbLemma::Become) {
        consume_verb(cursor, VerbLemma::Become)?;
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
        if verb == SourceEventVerb::BecameBlocked && peek_is(cursor, 0, "by") {
            cursor.consume("by")?;
            read_counterpart(cursor, SourceEventVerb::BecameBlocked, modifiers)?;
        }
        return Ok(verb);
    }
    Err(cursor.fail(
        "enters, dies, attacks, blocks, leaves the battlefield, is put into a graveyard, or becomes STATE",
    ))
}

/// Reads `SOURCE VERB [or VERB]... [EVENT MODIFIER]` after the trigger word.
/// The enclosing envelope owns the trigger word, the comma and the body.
pub(in crate::oracle::canonical) fn recognize_source_event_union(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEventUnionNode> {
    cursor.enter("event.source.subject");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    let mut modifiers = Vec::new();
    let mut verbs = vec![recognize_source_event_verb(cursor, &mut modifiers)?];
    while peek_is(cursor, 0, "or") {
        cursor.enter("event.source.union");
        cursor.consume("or")?;
        let verb = recognize_source_event_verb(cursor, &mut modifiers)?;
        cursor.enter("event.source.union-distinct");
        cursor.require(!verbs.contains(&verb), "a distinct event in the union")?;
        verbs.push(verb);
    }
    // `blocks or becomes blocked by QUERY`: the trailing counterpart is shared
    // by a bare preceding `blocks` (it blocks such a creature or becomes
    // blocked by one). A separately qualified `blocks QUERY` keeps its own.
    if verbs.len() > 1
        && let Some(SourceEventModifier::Counterpart(shared)) = modifiers
            .iter()
            .find(|modifier| matches!(modifier, SourceEventModifier::Counterpart(node) if node.verb == *verbs.last().expect("verb")))
            .cloned()
    {
        for verb in &verbs[..verbs.len() - 1] {
            let qualified = modifiers.iter().any(|modifier| {
                matches!(modifier, SourceEventModifier::Counterpart(node) if node.verb == *verb)
            });
            if *verb == SourceEventVerb::Blocked && !qualified {
                modifiers.push(SourceEventModifier::Counterpart(EventCounterpartNode {
                    verb: *verb,
                    ..shared.clone()
                }));
            }
        }
    }
    // Generic event modifiers: the negated companion event and the
    // turn-ordinal qualifier. They are typed attributes of the pattern.
    cursor.enter("event.source.modifier");
    if peek_is(cursor, 0, "and")
        && (peek_is(cursor, 1, "isn't") || peek_is(cursor, 1, "isn’t"))
        && peek_is(cursor, 2, "blocked")
    {
        cursor.advance_to(cursor.position() + 3)?;
        modifiers.push(SourceEventModifier::NotBlocked);
    }
    if peek_is(cursor, 0, "for")
        && peek_is(cursor, 1, "the")
        && peek_is(cursor, 2, "first")
        && peek_is(cursor, 3, "time")
    {
        cursor.consume_phrase(&["for", "the", "first", "time", "each", "turn"])?;
        modifiers.push(SourceEventModifier::FirstTimeEachTurn);
    }
    cursor.enter("event.source.complete");
    Ok(SourceEventUnionNode {
        source,
        verbs,
        modifiers,
    })
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

#[cfg(test)]
#[path = "event_clause_tests.rs"]
mod event_clause_tests;
