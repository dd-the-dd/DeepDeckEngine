//! Events whose subject is the ability source itself (CR 603.2, 603.6c,
//! 603.10a), as an ordered union of independent event verbs:
//!
//! ```text
//! SourceEvent := SOURCE SourceVerb ("or" SourceVerb)*
//! SourceVerb  := "enters" ["the battlefield"] | "dies" | "attacks" | "blocks"
//!              | "leaves the battlefield"
//!              | "is put into a graveyard from the battlefield"
//!              | "becomes" ("tapped" | "untapped" | "blocked")
//!              | "becomes the target of" STACK-OBJECT
//!              | "exploits" COUNTERPART
//! SourceEvent ModifierTail   -- the shared tail of `event_modifiers.rs`
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
    /// `is put into a graveyard from anywhere`.
    PutIntoGraveyardFromAnywhere,
    /// `is put into your graveyard from your library|hand`: the card's own
    /// arrival from that zone (its owner's graveyard, CR 400.3).
    PutIntoGraveyardFromZone(&'static str),
    /// `is put into exile from the battlefield` (CR 603.10a look-back).
    PutIntoExileFromBattlefield,
    BecameTapped,
    BecameUntapped,
    BecameBlocked,
    /// `becomes the target of STACK-OBJECT` (CR 115.1), always with its
    /// targeting stack-object selector (`SourceEventModifier::TargetingStack`).
    BecameTarget,
    /// `exploits a creature` (CR 702.110b), always with its counterpart.
    Exploited,
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
    /// `during your turn` / `during an opponent's turn` (CR 500.1).
    DuringTurn(super::EventTurnOwner),
    /// The spell or ability of `becomes the target of STACK-OBJECT`.
    TargetingStack(super::TargetingStackSelectorNode),
}

impl SourceEventModifier {
    /// The clause-level modifiers of the shared tail, as source modifiers.
    pub(in crate::oracle::canonical) fn from_tail(tail: super::EventModifierTail) -> Vec<Self> {
        let mut modifiers = Vec::new();
        if tail.not_blocked {
            modifiers.push(Self::NotBlocked);
        }
        if tail.first_time_each_turn {
            modifiers.push(Self::FirstTimeEachTurn);
        }
        if let Some(owner) = tail.turn_owner {
            modifiers.push(Self::DuringTurn(owner));
        }
        modifiers
    }
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
    let query = super::recognize_object_query(cursor)?;
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
        if super::consume_alone_modifier(cursor)? {
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
    if verb_at(cursor, VerbLemma::Exploit) {
        // CR 702.110b: the source's controller sacrificed the counterpart as
        // the source's exploit ability resolved.
        consume_verb(cursor, VerbLemma::Exploit)?;
        read_counterpart(cursor, SourceEventVerb::Exploited, modifiers)?;
        return Ok(SourceEventVerb::Exploited);
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
        if peek_is(cursor, 0, "exile") {
            cursor.enter("event.source.put-into-exile.origin");
            cursor.consume("exile")?;
            if peek_is(cursor, 0, "from") {
                cursor.consume_phrase(&["from", "the", "battlefield"])?;
            }
            return Ok(SourceEventVerb::PutIntoExileFromBattlefield);
        }
        cursor.enter("event.source.put-into-graveyard.zone");
        // `your graveyard` is the card's own (its owner's) graveyard; from
        // the battlefield it would add a controller relation without matcher.
        let own = peek_is(cursor, 0, "your");
        if own {
            cursor.consume_phrase(&["your", "graveyard"])?;
        } else {
            cursor.consume_phrase(&["a", "graveyard"])?;
        }
        cursor.enter("event.source.put-into-graveyard.origin");
        if peek_is(cursor, 0, "from") && peek_is(cursor, 1, "anywhere") {
            cursor.consume_phrase(&["from", "anywhere"])?;
            return Ok(SourceEventVerb::PutIntoGraveyardFromAnywhere);
        }
        if own && peek_is(cursor, 0, "from") && peek_is(cursor, 1, "your") {
            cursor.consume_phrase(&["from", "your"])?;
            let zone = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("library") => "library",
                Some("hand") => "hand",
                _ => return Err(cursor.fail("library or hand")),
            };
            cursor.consume(zone)?;
            return Ok(SourceEventVerb::PutIntoGraveyardFromZone(zone));
        }
        cursor.require(
            !own,
            "a graveyard (an owner relation from the battlefield has no matcher)",
        )?;
        cursor.consume_phrase(&["from", "the", "battlefield"])?;
        return Ok(SourceEventVerb::PutIntoGraveyardFromBattlefield);
    }
    if verb_at(cursor, VerbLemma::Become) {
        consume_verb(cursor, VerbLemma::Become)?;
        // The targeting frame of `become` (CR 115.1): the stack object is
        // a typed attribute; a source observer takes any controller.
        if super::targeting_frame_starts(cursor, 0) {
            super::consume_targeting_relation(cursor)?;
            let stack = super::recognize_targeting_stack_selector(
                cursor,
                super::TargetingControllerPolicy::Optional,
            )?;
            modifiers.push(SourceEventModifier::TargetingStack(stack));
            return Ok(SourceEventVerb::BecameTarget);
        }
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
    // The shared clause modifier tail (the negated companion event, the
    // turn ordinal, the turn owner): typed attributes of the pattern.
    modifiers.extend(SourceEventModifier::from_tail(
        super::read_event_modifier_tail(cursor)?,
    ));
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
