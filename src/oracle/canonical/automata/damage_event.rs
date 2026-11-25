//! The complement of a damage-dealing event verb (CR 120.1, 120.2):
//!
//! ```text
//! DealtComplement := ["combat" | "noncombat"] "damage" ["to" Recipient]
//! Recipient       := Actor                       -- you, a player, an opponent
//!                  | "one" "of" "your" "opponents" -- partitive of `an opponent`
//!                  | ("a" | "an") PermanentQuery -- a creature, a planeswalker, ...
//! Recipients      := Recipient ("or" (Recipient | ElidedRecipient))*
//! ElidedRecipient := "player" | "opponent" | PermanentQuery   -- the shared article
//! ```
//!
//! Recipient alternatives (`to a player or planeswalker`) are alternative
//! occurrences of the event: the enclosing event clause splits them into one
//! occurrence per recipient, so no consumer reads a recipient union.
//!
//! The subject that deals the damage (the source, the attached object or an
//! object query) belongs to the enclosing event; damage kind and recipient
//! are independent attributes shared by every subject.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventActorScope, ObjectKind, ObjectQueryNode, recognize_object_query,
    try_recognize_event_actor_scope,
};

/// Which damage an event, pattern or replacement admits (CR 120.2, 510.2).
/// One modifier type for every damage role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageKind {
    Any,
    Combat,
    Noncombat,
}

/// The optional `combat` / `noncombat` modifier of the damage noun.
pub(in crate::oracle::canonical) fn recognize_damage_kind(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageKind> {
    cursor.enter("damage.kind");
    let kind = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("combat") => DamageKind::Combat,
        Some("noncombat") => DamageKind::Noncombat,
        _ => return Ok(DamageKind::Any),
    };
    cursor.advance_to(cursor.position() + 1)?;
    Ok(kind)
}

/// `[combat|noncombat] damage`.
pub(in crate::oracle::canonical) fn recognize_dealt_damage_complement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageKind> {
    let kind = recognize_damage_kind(cursor)?;
    cursor.enter("event.dealt-damage.noun");
    cursor.consume("damage")?;
    Ok(kind)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageRecipient {
    Player(EventActorScope),
    Permanent(ObjectQueryNode),
    /// `one or more CREATURES`: the permanents one damage event of the
    /// source dealt damage to, observed once per source and event
    /// (CR 603.2c); no single damaged permanent is bound.
    PermanentBatch(ObjectQueryNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DamageDealtComplement {
    pub kind: DamageKind,
    /// `None` for `deals damage` without a recipient.
    pub recipient: Option<DamageRecipient>,
    /// Further recipient alternatives in written order (`or planeswalker`);
    /// empty for one recipient. The event clause splits them before any
    /// consumer reads `recipient`.
    pub alternatives: Vec<DamageRecipient>,
}

impl DamageDealtComplement {
    /// The established `combat damage to a player` shape.
    pub(in crate::oracle::canonical) fn is_combat_damage_to_any_player(&self) -> bool {
        self.kind == DamageKind::Combat
            && self.recipient == Some(DamageRecipient::Player(EventActorScope::AnyPlayer))
            && self.alternatives.is_empty()
    }

    /// One complement per recipient alternative, in written order; a
    /// complement without alternatives is returned alone.
    pub(in crate::oracle::canonical) fn split_recipients(&self) -> Vec<Self> {
        std::iter::once(self.recipient.clone())
            .chain(self.alternatives.iter().cloned().map(Some))
            .map(|recipient| Self {
                kind: self.kind,
                recipient,
                alternatives: Vec::new(),
            })
            .collect()
    }
}

/// Whether another recipient alternative follows `or` at `offset`: a player
/// actor, an article, an elided `player`/`opponent`, or a card type sharing
/// the first recipient's article. A lexical probe.
fn recipient_alternative_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        ["you", "a", "an", "player", "opponent"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    }) || cursor
        .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
        .first()
        .is_some_and(|token| token.token_type == "typeWord")
}

/// One recipient: a player actor or an `a`/`an` permanent query. After `or`
/// the article may be elided (`a player or planeswalker`).
fn recognize_damage_recipient(
    cursor: &mut OracleTokenCursor<'_>,
    elided_article: bool,
) -> MachineResult<DamageRecipient> {
    cursor.enter("event.dealt-damage.recipient");
    if let Some(actor) = try_recognize_event_actor_scope(cursor)? {
        return Ok(DamageRecipient::Player(actor));
    }
    // `one of your opponents` (CR 102.3): the partitive names one member
    // of the ability controller's opponents, the relation of `an opponent`.
    let partitive = ["one", "of", "your", "opponents"];
    if !elided_article
        && partitive.iter().enumerate().all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
    {
        cursor.enter("event.dealt-damage.recipient-partitive");
        cursor.consume_phrase(&partitive)?;
        return Ok(DamageRecipient::Player(EventActorScope::Opponent));
    }
    // `one or more CREATURES [you control]` (CR 603.2c): the shared object
    // collection; a batch is never an `or` alternative.
    if !elided_article
        && ["one", "or", "more"]
            .iter()
            .enumerate()
            .all(|(offset, word)| {
                cursor
                    .peek_ahead(offset)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            })
    {
        cursor.enter("event.dealt-damage.recipient-batch");
        cursor.consume_phrase(&["one", "or", "more"])?;
        let query = super::recognize_object_collection(cursor)?;
        cursor.enter("event.dealt-damage.recipient-domain");
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent)
                && query.predicate.has_nominal_head()
                && !query.exclude_source,
            "a permanent batch recipient with a nominal head",
        )?;
        return Ok(DamageRecipient::PermanentBatch(query));
    }
    if elided_article {
        for (word, actor) in [
            ("player", EventActorScope::AnyPlayer),
            ("opponent", EventActorScope::Opponent),
        ] {
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            {
                cursor.consume(word)?;
                return Ok(DamageRecipient::Player(actor));
            }
        }
    }
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an");
    match article {
        Some(article) => cursor.consume(&article)?,
        None if elided_article => {}
        None => {
            return Err(cursor.fail("a player, an opponent, you, or `a`/`an` permanent"));
        }
    }
    cursor.enter("event.dealt-damage.recipient-query");
    let query = recognize_object_query(cursor)?;
    cursor.enter("event.dealt-damage.recipient-domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && query.predicate.has_nominal_head(),
        "a permanent recipient with a nominal head",
    )?;
    Ok(DamageRecipient::Permanent(query))
}

/// Reads the complement after `deal`/`deals`. A recipient is optional: the
/// enclosing envelope owns the following comma.
pub(in crate::oracle::canonical) fn recognize_damage_dealt_complement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageDealtComplement> {
    let kind = recognize_dealt_damage_complement(cursor)?;
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("to"))
    {
        return Ok(DamageDealtComplement {
            kind,
            recipient: None,
            alternatives: Vec::new(),
        });
    }
    cursor.enter("event.dealt-damage.recipient-relation");
    cursor.consume("to")?;
    let recipient = recognize_damage_recipient(cursor, false)?;
    let mut alternatives = Vec::new();
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && recipient_alternative_starts_at(cursor, 1)
    {
        cursor.enter("event.dealt-damage.recipient-union");
        cursor.consume("or")?;
        let alternative = recognize_damage_recipient(cursor, true)?;
        cursor.enter("event.dealt-damage.recipient-union-distinct");
        cursor.require(
            alternative != recipient && !alternatives.contains(&alternative),
            "distinct recipient alternatives",
        )?;
        alternatives.push(alternative);
    }
    Ok(DamageDealtComplement {
        kind,
        recipient: Some(recipient),
        alternatives,
    })
}

#[cfg(test)]
#[path = "damage_event_tests.rs"]
mod tests;
