//! The complement of a damage-dealing event verb (CR 120.1, 120.2):
//!
//! ```text
//! DealtComplement := ["combat" | "noncombat"] "damage" ["to" Recipient]
//! Recipient       := Actor                       -- you, a player, an opponent
//!                  | ("a" | "an") PermanentQuery -- a creature, a planeswalker, ...
//! ```
//!
//! The subject that deals the damage (the source, the attached object or an
//! object query) belongs to the enclosing event; damage kind and recipient
//! are independent attributes shared by every subject.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventActorScope, ObjectKind, ObjectQueryNode, recognize_object_query_before_separator,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DamageDealtComplement {
    pub kind: DamageKind,
    /// `None` for `deals damage` without a recipient.
    pub recipient: Option<DamageRecipient>,
}

impl DamageDealtComplement {
    /// The established `combat damage to a player` shape.
    pub(in crate::oracle::canonical) fn is_combat_damage_to_any_player(&self) -> bool {
        self.kind == DamageKind::Combat
            && self.recipient == Some(DamageRecipient::Player(EventActorScope::AnyPlayer))
    }
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
        });
    }
    cursor.enter("event.dealt-damage.recipient-relation");
    cursor.consume("to")?;
    cursor.enter("event.dealt-damage.recipient");
    if let Some(actor) = try_recognize_event_actor_scope(cursor)? {
        return Ok(DamageDealtComplement {
            kind,
            recipient: Some(DamageRecipient::Player(actor)),
        });
    }
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an")
        .ok_or_else(|| cursor.fail("a player, an opponent, you, or `a`/`an` permanent"))?;
    cursor.consume(&article)?;
    cursor.enter("event.dealt-damage.recipient-query");
    let query = recognize_object_query_before_separator(cursor)?;
    cursor.enter("event.dealt-damage.recipient-domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && query.predicate.has_nominal_head(),
        "a permanent recipient with a nominal head",
    )?;
    Ok(DamageDealtComplement {
        kind,
        recipient: Some(DamageRecipient::Permanent(query)),
    })
}

#[cfg(test)]
#[path = "damage_event_tests.rs"]
mod tests;
