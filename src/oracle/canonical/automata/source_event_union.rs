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
    /// `is turned face up` (CR 708.8): the passive of the `turn` lemma with
    /// its `face up` state complement.
    TurnedFaceUp,
    /// `becomes monstrous|renowned` (CR 701.37b, 702.112b): the source gains
    /// a designation an action gives, read by the status registry.
    BecameDesignated(super::PermanentDesignation),
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
    /// `enters from ZONE`: the zone the entering source left.
    EnteredFrom(super::EntryOriginNode),
    /// `during combat` (CR 506.1): the event happens in a combat step.
    DuringCombat,
    /// `attacks RECIPIENT [or RECIPIENT]`: the alternatives of the object the
    /// source attacks (CR 506.2, 508.1b), the attack verb's own argument.
    AttackedRecipients(Vec<super::AttackedRecipientNode>),
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
        if tail.during_combat {
            modifiers.push(Self::DuringCombat);
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

fn verb_at(cursor: &OracleTokenCursor<'_>, lemma: VerbLemma, agreement: VerbAgreement) -> bool {
    lemma_at(cursor, lemma, agreement)
}

fn consume_verb(
    cursor: &mut OracleTokenCursor<'_>,
    lemma: VerbLemma,
    agreement: VerbAgreement,
) -> MachineResult<()> {
    consume_lemma(cursor, lemma, agreement)
}

/// The passive auxiliary `be` agreeing with the source subject.
fn passive_auxiliary(agreement: VerbAgreement) -> &'static str {
    match agreement {
        VerbAgreement::ThirdPersonSingular => "is",
        VerbAgreement::Base => "are",
    }
}

/// `AUXILIARY turned face up` at the cursor: the passive participle of the
/// `turn` lemma (lexicon data) with its state complement. A lexical probe.
pub(in crate::oracle::canonical) fn turned_face_up_starts(
    cursor: &OracleTokenCursor<'_>,
    auxiliary: &str,
) -> bool {
    peek_is(cursor, 0, auxiliary)
        && cursor.peek_ahead(1).is_some_and(|raw| {
            super::is_verb_form(raw, VerbLemma::Turn, super::VerbForm::PastParticiple)
        })
        && peek_is(cursor, 2, "face")
        && peek_is(cursor, 3, "up")
}

/// Consumes `AUXILIARY turned face up` (CR 708.8).
pub(in crate::oracle::canonical) fn consume_turned_face_up(
    cursor: &mut OracleTokenCursor<'_>,
    auxiliary: &str,
) -> MachineResult<()> {
    cursor.enter("event.turned-face-up");
    cursor.consume(auxiliary)?;
    let participle = cursor
        .peek()
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("the passive participle of `turn`"))?;
    cursor.consume(&participle)?;
    cursor.consume_phrase(&["face", "up"])
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

/// The lemmas that start a source event verb in the singular agreement of
/// `this PERMANENT`, or the passive auxiliary (`is put into`, `is turned face
/// up`). A lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn source_verb_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    source_verb_starts_in(cursor, VerbAgreement::ThirdPersonSingular)
}

fn source_verb_starts_in(cursor: &OracleTokenCursor<'_>, agreement: VerbAgreement) -> bool {
    peek_is(cursor, 0, passive_auxiliary(agreement))
        || [
            VerbLemma::Enter,
            VerbLemma::Die,
            VerbLemma::Attack,
            VerbLemma::Block,
            VerbLemma::Exploit,
            VerbLemma::Leave,
            VerbLemma::Become,
        ]
        .into_iter()
        .any(|lemma| verb_at(cursor, lemma, agreement))
}

fn recognize_source_event_verb(
    cursor: &mut OracleTokenCursor<'_>,
    modifiers: &mut Vec<SourceEventModifier>,
    agreement: VerbAgreement,
) -> MachineResult<SourceEventVerb> {
    let auxiliary = passive_auxiliary(agreement);
    cursor.enter("event.source.verb");
    // Each verb is a lexicon lemma in the subject's agreement (the shared
    // clause); the frame after it is read here as event operands.
    if verb_at(cursor, VerbLemma::Enter, agreement) {
        consume_verb(cursor, VerbLemma::Enter, agreement)?;
        if peek_is(cursor, 0, "the") && peek_is(cursor, 1, "battlefield") {
            cursor.consume_phrase(&["the", "battlefield"])?;
        }
        if super::entry_origin_starts(cursor) {
            modifiers.push(SourceEventModifier::EnteredFrom(
                super::recognize_entry_origin(cursor)?,
            ));
        }
        return Ok(SourceEventVerb::Entered);
    }
    if verb_at(cursor, VerbLemma::Die, agreement) {
        consume_verb(cursor, VerbLemma::Die, agreement)?;
        return Ok(SourceEventVerb::Died);
    }
    if verb_at(cursor, VerbLemma::Attack, agreement) {
        consume_verb(cursor, VerbLemma::Attack, agreement)?;
        // The attacked player or permanent is the attack's own argument,
        // read by the shared recipient reader (CR 506.2).
        if super::attacked_recipient_starts(cursor) {
            modifiers.push(SourceEventModifier::AttackedRecipients(
                super::recognize_attacked_recipients(cursor)?,
            ));
        }
        if super::consume_alone_modifier(cursor)? {
            modifiers.push(SourceEventModifier::AttackingAlone);
        }
        return Ok(SourceEventVerb::Attacked);
    }
    if verb_at(cursor, VerbLemma::Block, agreement) {
        consume_verb(cursor, VerbLemma::Block, agreement)?;
        if peek_is(cursor, 0, "a") || peek_is(cursor, 0, "an") || peek_is(cursor, 0, "one") {
            read_counterpart(cursor, SourceEventVerb::Blocked, modifiers)?;
        }
        return Ok(SourceEventVerb::Blocked);
    }
    if verb_at(cursor, VerbLemma::Exploit, agreement) {
        // CR 702.110b: the source's controller sacrificed the counterpart as
        // the source's exploit ability resolved.
        consume_verb(cursor, VerbLemma::Exploit, agreement)?;
        read_counterpart(cursor, SourceEventVerb::Exploited, modifiers)?;
        return Ok(SourceEventVerb::Exploited);
    }
    if verb_at(cursor, VerbLemma::Leave, agreement) {
        cursor.enter("event.source.leaves");
        consume_verb(cursor, VerbLemma::Leave, agreement)?;
        cursor.consume_phrase(&["the", "battlefield"])?;
        return Ok(SourceEventVerb::LeftBattlefield);
    }
    if turned_face_up_starts(cursor, auxiliary) {
        consume_turned_face_up(cursor, auxiliary)?;
        return Ok(SourceEventVerb::TurnedFaceUp);
    }
    if peek_is(cursor, 0, auxiliary) && peek_is(cursor, 1, "put") {
        // The passive of `put` (`is put into a graveyard from the battlefield`).
        cursor.enter("event.source.put-into-graveyard");
        cursor.consume(auxiliary)?;
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
    if verb_at(cursor, VerbLemma::Become, agreement) {
        consume_verb(cursor, VerbLemma::Become, agreement)?;
        // The targeting frame of `become` (CR 115.1): the stack object is
        // a typed attribute; a source observer takes any controller.
        if super::targeting_frame_starts(cursor, 0) {
            super::consume_targeting_relation(cursor)?;
            let stack = super::recognize_targeting_stack_selector(cursor)?;
            modifiers.push(SourceEventModifier::TargetingStack(stack));
            return Ok(SourceEventVerb::BecameTarget);
        }
        cursor.enter("event.source.becomes");
        // A designation an action gives (the shared status registry).
        if let Some(designation) = cursor
            .peek()
            .and_then(super::PermanentDesignation::from_adjective)
            .filter(|designation| designation.given_by_becomes())
        {
            let raw = cursor.peek().map(str::to_string).unwrap_or_default();
            cursor.consume(&raw)?;
            return Ok(SourceEventVerb::BecameDesignated(designation));
        }
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
    let named = !peek_is(cursor, 0, "this");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    // A card name agrees in its own grammatical number (`When The Notary
    // Hobbits enter`); `this PERMANENT` is singular.
    let agreement = if named
        && !source_verb_starts_in(cursor, VerbAgreement::ThirdPersonSingular)
        && source_verb_starts_in(cursor, VerbAgreement::Base)
    {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    recognize_source_event_predicate_in(cursor, source, agreement)
}

/// The verbs, modifiers and tail of a source event after its (already read)
/// subject. A subject union (`SOURCE or another QUERY VERB`) reads the same
/// predicate for its source branch.
pub(in crate::oracle::canonical) fn recognize_source_event_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    source: LocalObjectReference,
) -> MachineResult<SourceEventUnionNode> {
    recognize_source_event_predicate_in(cursor, source, VerbAgreement::ThirdPersonSingular)
}

fn recognize_source_event_predicate_in(
    cursor: &mut OracleTokenCursor<'_>,
    source: LocalObjectReference,
    agreement: VerbAgreement,
) -> MachineResult<SourceEventUnionNode> {
    let mut modifiers = Vec::new();
    let mut verbs = vec![recognize_source_event_verb(
        cursor,
        &mut modifiers,
        agreement,
    )?];
    while peek_is(cursor, 0, "or") {
        // `or` continues this verb union only before another source verb;
        // otherwise it belongs to the enclosing occurrence union (`SOURCE
        // enters or another creature dies`), which reads it after the tail.
        let before_or = cursor.checkpoint();
        cursor.enter("event.source.union");
        cursor.consume("or")?;
        if !source_verb_starts_in(cursor, agreement) {
            cursor.rollback(before_or);
            break;
        }
        let verb = recognize_source_event_verb(cursor, &mut modifiers, agreement)?;
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

/// Reads the state-trigger condition `SOURCE has no COUNTERS on it`,
/// `SOURCE has N or more COUNTERS on it` or any other count threshold of the
/// shared condition language (CR 603.8): the possession frame of
/// the `have` lemma with a counter quantity and its holder anaphor. The
/// enclosing clause owns the trigger word and the boundary.
pub(in crate::oracle::canonical) fn recognize_source_counter_state_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<super::SourceCounterStateNode> {
    cursor.enter("event.counter-state.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.counter-state.verb");
    let verb = cursor
        .peek()
        .filter(|raw| {
            super::is_verb_form(raw, VerbLemma::Have, super::VerbForm::ThirdPersonSingular)
        })
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("the possession verb `has`"))?;
    cursor.consume(&verb)?;
    cursor.enter("event.counter-state.quantity");
    // The shared count-threshold grammar of the condition language (`no`,
    // `N or more|fewer`, `at least|most N`, `exactly N`).
    let threshold = super::recognize_count_threshold(cursor, false)?;
    let comparison = match (threshold.operator, threshold.value) {
        (super::ThresholdOperator::AtMost, 0) => super::CounterStateComparison::None,
        (super::ThresholdOperator::AtLeast, minimum) => {
            cursor.require(minimum >= 1, "a positive counter threshold")?;
            super::CounterStateComparison::AtLeast(minimum)
        }
        (super::ThresholdOperator::AtMost, maximum) => {
            super::CounterStateComparison::AtMost(maximum)
        }
        (super::ThresholdOperator::Exactly, value) => super::CounterStateComparison::Exactly(value),
    };
    cursor.enter("event.counter-state.counter");
    let counter = super::recognize_counter_name(cursor)?;
    let noun = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("counters") || raw.eq_ignore_ascii_case("counter"))
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("`counters`"))?;
    cursor.consume(&noun)?;
    cursor.enter("event.counter-state.holder");
    cursor.consume_phrase(&["on", "it"])?;
    cursor.enter("event.counter-state.complete");
    Ok(super::SourceCounterStateNode {
        source,
        counter,
        comparison,
    })
}

/// Reads `SOURCE and at least N other COLLECTION attack` (battalion, CR
/// 702.101a style): one attack declaration containing the source and at
/// least N other attackers read by the shared collection query. The verb is
/// the `attack` lemma in the plural agreement of the coordinated subject.
pub(in crate::oracle::canonical) fn recognize_source_attack_declaration(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<super::AttackDeclarationNode> {
    cursor.enter("event.attack-declaration.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.attack-declaration.coordination");
    cursor.consume("and")?;
    cursor.consume_phrase(&["at", "least"])?;
    cursor.enter("event.attack-declaration.minimum");
    let super::NumericAtom::Integer(minimum) = super::recognize_numeric_atom(cursor)? else {
        return Err(cursor.fail("a fixed number of other attackers"));
    };
    cursor.require(minimum >= 1, "a positive number of other attackers")?;
    cursor.enter("event.attack-declaration.companions");
    // The collection frame reads `other` once: the source is excluded from
    // the companions it is coordinated with.
    let query = super::recognize_object_collection(cursor)?;
    cursor.require(
        query.exclude_source,
        "`other` attackers coordinated with the source",
    )?;
    cursor.enter("event.attack-declaration.verb");
    consume_lemma(cursor, VerbLemma::Attack, VerbAgreement::Base)?;
    Ok(super::AttackDeclarationNode {
        attacker: super::EventActorScope::Controller,
        attackers: super::QuantifiedEventSubjectNode {
            multiplicity: super::EventMultiplicity::PerEvent { minimum },
            exclude_source: true,
            query,
        },
        source: Some(source),
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
