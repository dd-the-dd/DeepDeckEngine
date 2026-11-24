//! Temporary rules a resolving effect creates for players and for the game
//! (CR 611.2a): a continuous effect whose body is a player rule of the
//! shared static vocabulary, bounded by an explicit lifetime.
//!
//! ```text
//! TemporaryRule := Allowance Duration
//!                | PlayerSubject PlayerPredicate TemporaryDuration
//!                | "damage" PROHIBITION "be" "prevented" TemporaryDuration
//!                | "the" "next" SPELLS "you" "cast" "this" "turn" NextSpellRule
//!                | "you" "may" "cast" SPELLS "this" "turn"
//!                  "as" "though" "they" "had" "flash"        (CR 702.8a, 601.3)
//! Allowance     := PLAYER "may" "play" N "additional" ("land" | "lands")
//! PlayerSubject / PlayerPredicate := `static_player_predicate.rs`
//!                | "you" "have" "no" "maximum" "hand" "size"   (CR 402.2)
//! TemporaryDuration := "this" "turn" | "until" "your" "next" "turn"
//!                    | "for" "the" "rest" "of" "the" "game"
//! NextSpellRule := PROHIBITION "be" "countered"
//!                | "can" "be" "cast" "as" "though" "it" "had" "flash"
//! ```
//!
//! `the next SPELLS you cast this turn` is one spell-history ordinal (CR
//! 601.2i): the first spell matching the shared spell query that the
//! controller casts this turn. Its rule applies to that spell and is
//! consumed by it.
//!
//! The subject and predicate are the readers of a static ability; only the
//! written lifetime makes the rule temporary. Without one the clause is a
//! static ability (`Your opponents can't cast spells during your turn.`)
//! and this child rolls back. Lifetime and sentence boundary belong to the
//! enclosing sequence for the allowance header, as before.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

/// The rule a temporary effect applies while it lasts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TemporaryRuleNode {
    /// `you may play N additional lands` (CR 305.2).
    Allowance(PlayerModifierNode),
    /// A static player predicate over a player subject (`Players can't gain
    /// life`, `Your opponents can't cast noncreature spells`).
    Players {
        subject: StaticPlayerSubjectNode,
        predicate: StaticPlayerPredicateNode,
    },
    /// `you have no maximum hand size` (CR 402.2).
    NoMaximumHandSize,
    /// `damage can't be prevented` (CR 615.12): a game rule over every
    /// damage event while it lasts.
    DamageCantBePrevented,
    /// `the next SPELLS you cast this turn MODIFICATION`: a one-shot rule
    /// consumed by the first matching cast of the controller.
    NextSpell {
        spells: ObjectPredicate,
        modification: NextSpellModification,
    },
    /// `you may cast SPELLS this turn as though they had flash`: the
    /// controller casts every matching spell with instant timing while the
    /// rule lasts (CR 702.8a, 601.3). Costs and zones are unchanged.
    FlashCasting { spells: ObjectPredicate },
}

/// What a next-spell rule does to the spell it applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NextSpellModification {
    /// `can't be countered` (CR 701.6).
    CantBeCountered,
    /// `can be cast as though it had flash` (CR 702.8a, 601.3e).
    CastAsThoughFlash,
}

/// How long a temporary rule lasts (CR 611.2a-b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TemporaryRuleDuration {
    /// The shared effect duration (`this turn`, `until your next turn`).
    Effect(EffectDurationNode),
    /// `for the rest of the game`.
    RestOfGame,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryPlayerPermissionNode {
    pub rule: TemporaryRuleNode,
    pub duration: TemporaryRuleDuration,
}

pub(in crate::oracle::canonical) enum TemporaryPlayerPermissionParseAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(TemporaryPlayerPermissionNode),
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

/// The lifetime words a temporary rule may start with.
fn temporary_duration_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    (word_at(cursor, 0, "this") && word_at(cursor, 1, "turn"))
        || word_at(cursor, 0, "until")
        || (word_at(cursor, 0, "for") && word_at(cursor, 1, "the") && word_at(cursor, 2, "rest"))
}

/// `this turn`, `until your next turn`, `until end of turn`, `until the
/// end of your next turn` or `for the rest of the game`.
fn recognize_temporary_duration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TemporaryRuleDuration> {
    cursor.enter("temporary-rule.duration");
    if word_at(cursor, 0, "for") {
        cursor.consume_phrase(&["for", "the", "rest", "of", "the", "game"])?;
        return Ok(TemporaryRuleDuration::RestOfGame);
    }
    // The shared window reader: a temporary rule lasts through any turn
    // boundary the permission windows read (CR 611.2a, 611.2b).
    super::recognize_play_window_duration(cursor).map(TemporaryRuleDuration::Effect)
}

/// The lifetime after a complete rule body: absent, the clause is a static
/// ability (rolled back); started but malformed, the temporary rule owns
/// the failure.
fn close_temporary_rule(
    cursor: &mut OracleTokenCursor<'_>,
    checkpoint: super::super::machine::MachineCheckpoint,
    rule: TemporaryRuleNode,
) -> TemporaryPlayerPermissionParseAttempt {
    if !temporary_duration_starts(cursor) {
        let failure = cursor.fail("a temporary rule's lifetime (`this turn`, `until ...`)");
        cursor.rollback(checkpoint);
        return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
    }
    match recognize_temporary_duration(cursor) {
        Ok(duration) => {
            TemporaryPlayerPermissionParseAttempt::Parsed(TemporaryPlayerPermissionNode {
                rule,
                duration,
            })
        }
        Err(failure) => TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure),
    }
}

/// The lifetime and sentence boundary belong to the caller, not this header.
pub(in crate::oracle::canonical) fn recognize_additional_land_allowance_header(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    cursor.enter("modifier.additional-land-play.player");
    let player = recognize_player_reference(cursor, None)?;
    cursor.enter("modifier.additional-land-play.permission");
    cursor.consume("may")?;
    cursor.enter("modifier.additional-land-play.action");
    cursor.consume("play")?;
    cursor.enter("modifier.additional-land-play.quantity");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("modifier.additional-land-play.additional");
    cursor.consume("additional")?;
    cursor.enter("modifier.additional-land-play.land");
    let noun = if count == NumericExpression::Atom(NumericAtom::Integer(1)) {
        "land"
    } else {
        "lands"
    };
    cursor.consume(noun)?;
    Ok(PlayerModifierNode::AdditionalLandPlay { player, count })
}

/// A typed allowance plus a temporary-duration head belongs to the effect
/// parent. This bounded probe prevents the persistent static family stealing
/// that child. It reads shared tokens, restores the checkpoint, and never
/// guesses ownership from how far a failed child advanced.
pub(in crate::oracle::canonical) fn temporary_land_permission_owns_head(
    cursor: &mut OracleTokenCursor<'_>,
) -> bool {
    let checkpoint = cursor.checkpoint();
    let owns = recognize_additional_land_allowance_header(cursor).is_ok()
        && cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("this") || raw.eq_ignore_ascii_case("until")
        });
    cursor.rollback(checkpoint);
    owns
}

/// A temporary rule at the cursor: the allowance header, a player subject
/// with a player predicate, or `damage can't be prevented`, each followed
/// by its lifetime.
pub(in crate::oracle::canonical) fn try_recognize_temporary_player_permission(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryPlayerPermissionParseAttempt {
    if word_at(cursor, 0, "you") && word_at(cursor, 1, "may") && word_at(cursor, 2, "play") {
        return try_recognize_temporary_allowance(cursor);
    }
    if word_at(cursor, 0, "you")
        && word_at(cursor, 1, "may")
        && word_at(cursor, 2, "cast")
        && let Some(offsets) = flash_manner_offsets(cursor)
    {
        return try_recognize_temporary_flash_permission(cursor, offsets);
    }
    if word_at(cursor, 0, "damage")
        && super::player_prohibition_word_at(cursor, 1)
        && word_at(cursor, 2, "be")
        && word_at(cursor, 3, "prevented")
    {
        let checkpoint = cursor.checkpoint();
        cursor.enter("temporary-rule.damage-prevention");
        if let Err(failure) = cursor.advance_to(cursor.position() + 4) {
            return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
        }
        return close_temporary_rule(cursor, checkpoint, TemporaryRuleNode::DamageCantBePrevented);
    }
    if word_at(cursor, 0, "the") && word_at(cursor, 1, "next") && !word_at(cursor, 2, "time") {
        return try_recognize_next_spell_rule(cursor);
    }
    try_recognize_temporary_player_rule(cursor)
}

/// Offsets of the written lifetime `this turn` and of the timing manner
/// `as though they had flash` in the current sentence, when the lifetime
/// precedes the manner. Without both, `you may cast ...` is another frame's
/// (a card permission, a static permission or an optional instruction).
fn flash_manner_offsets(cursor: &OracleTokenCursor<'_>) -> Option<(usize, usize)> {
    let mut lifetime = None;
    let mut offset = 3;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if matches!(raw, "." | "(" | ";") {
            return None;
        }
        if lifetime.is_none()
            && word_at(cursor, offset, "this")
            && word_at(cursor, offset + 1, "turn")
        {
            lifetime = Some(offset);
        }
        if ["as", "though", "they", "had", "flash"]
            .iter()
            .enumerate()
            .all(|(at, word)| word_at(cursor, offset + at, word))
        {
            return lifetime.map(|lifetime| (lifetime, offset));
        }
        offset += 1;
    }
    None
}

/// `you may cast SPELLS this turn as though they had flash`. The spell query
/// is the shared spell collection, bounded by the written lifetime; once the
/// lifetime and manner are both present the frame owns its failures.
fn try_recognize_temporary_flash_permission(
    cursor: &mut OracleTokenCursor<'_>,
    (lifetime, manner): (usize, usize),
) -> TemporaryPlayerPermissionParseAttempt {
    if manner != lifetime + 2 {
        return TemporaryPlayerPermissionParseAttempt::NotClaimed;
    }
    let start = cursor.position();
    let parsed = (|| -> MachineResult<ObjectPredicate> {
        cursor.enter("temporary-rule.flash-permission.head");
        cursor.consume_phrase(&["you", "may", "cast"])?;
        cursor.enter("temporary-rule.flash-permission.spells");
        let query = cursor.bounded(start + lifetime, super::recognize_spell_cast_collection)?;
        cursor.require(
            query.caster.is_none() && !super::object_predicate_uses_x(&query.predicate),
            "a spell query without its own caster or an undeclared numeric producer",
        )?;
        cursor.enter("temporary-rule.flash-permission.lifetime");
        cursor.consume_phrase(&["this", "turn"])?;
        cursor.enter("temporary-rule.flash-permission.manner");
        cursor.consume_phrase(&["as", "though", "they", "had", "flash"])?;
        Ok(query.predicate)
    })();
    match parsed {
        Ok(spells) => {
            TemporaryPlayerPermissionParseAttempt::Parsed(TemporaryPlayerPermissionNode {
                rule: TemporaryRuleNode::FlashCasting { spells },
                duration: TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn),
            })
        }
        Err(failure) => TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure),
    }
}

/// The offset of `you cast this turn` closing a next-spell subject.
fn next_spell_relation_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (2..)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw != "." && raw != "," && raw != "(")
        })
        .find(|offset| {
            ["you", "cast", "this", "turn"]
                .iter()
                .enumerate()
                .all(|(at, word)| word_at(cursor, offset + at, word))
        })
}

/// `the next SPELLS you cast this turn RULE`. The spell query is the shared
/// object query read up to its relation; the relation and its period are
/// the ordinal's (the controller's casts this turn).
fn try_recognize_next_spell_rule(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryPlayerPermissionParseAttempt {
    let Some(relation) = next_spell_relation_offset(cursor) else {
        return TemporaryPlayerPermissionParseAttempt::NotClaimed;
    };
    let checkpoint = cursor.checkpoint();
    let end = cursor.position() + relation;
    let reject = |cursor: &mut OracleTokenCursor<'_>, failure: MachineFailure| {
        cursor.rollback(checkpoint);
        TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure)
    };
    cursor.enter("temporary-rule.next-spell.ordinal");
    if let Err(failure) = cursor.consume_phrase(&["the", "next"]) {
        return reject(cursor, failure);
    }
    let query = cursor.bounded(end, |query_cursor| {
        super::recognize_object_noun_query(query_cursor, super::ObjectQueryContext::ORDINARY)?
            .into_spell_cast_query(query_cursor)
    });
    let query = match query {
        Ok(query) => query,
        Err(failure) => return reject(cursor, failure),
    };
    cursor.enter("temporary-rule.next-spell.query");
    if !query.predicate.is_in_domain(ObjectKind::Spell)
        || query.caster.is_some()
        || super::object_predicate_uses_x(&query.predicate)
    {
        let failure = cursor.fail(
            "an explicit spell query without its own caster or an undeclared numeric producer",
        );
        return reject(cursor, failure);
    }
    cursor.enter("temporary-rule.next-spell.relation");
    if let Err(failure) = cursor.consume_phrase(&["you", "cast", "this", "turn"]) {
        return reject(cursor, failure);
    }
    // The ordinal subject is read: the rule after it belongs to this frame.
    match recognize_next_spell_modification(cursor) {
        Ok(modification) => {
            TemporaryPlayerPermissionParseAttempt::Parsed(TemporaryPlayerPermissionNode {
                rule: TemporaryRuleNode::NextSpell {
                    spells: query.predicate,
                    modification,
                },
                duration: TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn),
            })
        }
        Err(failure) => TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure),
    }
}

/// `can't be countered` / `can be cast as though it had flash`.
fn recognize_next_spell_modification(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NextSpellModification> {
    cursor.enter("temporary-rule.next-spell.rule");
    if super::player_prohibition_word_at(cursor, 0) {
        cursor.advance_to(cursor.position() + 1)?;
        cursor.consume_phrase(&["be", "countered"])?;
        return Ok(NextSpellModification::CantBeCountered);
    }
    if word_at(cursor, 0, "can") {
        cursor.consume_phrase(&["can", "be", "cast", "as", "though", "it", "had", "flash"])?;
        return Ok(NextSpellModification::CastAsThoughFlash);
    }
    Err(cursor.fail(
        "a next-spell rule the runtime applies: `can't be countered` or `can be cast as though it had flash`",
    ))
}

/// `you may play N additional lands DURATION`.
fn try_recognize_temporary_allowance(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryPlayerPermissionParseAttempt {
    let checkpoint = cursor.checkpoint();
    let modifier = match cursor.attempt(recognize_additional_land_allowance_header) {
        Ok(modifier) => modifier,
        Err(failure) => return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure),
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("on"))
    {
        let failure =
            cursor.fail("a temporary effect duration rather than a persistent per-turn allowance");
        cursor.rollback(checkpoint);
        return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
    }
    // A complete typed header commits its explicit temporary lifetime.
    cursor.enter("permission.temporary.duration");
    match recognize_effect_duration(cursor) {
        Ok(duration) => {
            TemporaryPlayerPermissionParseAttempt::Parsed(TemporaryPlayerPermissionNode {
                rule: TemporaryRuleNode::Allowance(modifier),
                duration: TemporaryRuleDuration::Effect(duration),
            })
        }
        Err(failure) => TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure),
    }
}

/// `PLAYERS PREDICATE DURATION`: the static frame's player subject and
/// player predicate. A subject this frame does not read, or a predicate the
/// static vocabulary does not have, is not claimed.
fn try_recognize_temporary_player_rule(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryPlayerPermissionParseAttempt {
    let Some((_, width)) = super::static_player_subject_width(cursor) else {
        return TemporaryPlayerPermissionParseAttempt::NotClaimed;
    };
    let mut probe = cursor.clone();
    if probe.advance_to(cursor.position() + width).is_err()
        || !super::static_player_predicate_starts(&probe)
    {
        return TemporaryPlayerPermissionParseAttempt::NotClaimed;
    }
    let checkpoint = cursor.checkpoint();
    let reading = match super::recognize_static_player_subject(cursor) {
        Ok(reading) => reading,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
        }
    };
    cursor.enter("temporary-rule.player-subject");
    // An attached permanent's controller exists only for an Aura or
    // Equipment's static ability.
    if reading.subject == StaticPlayerSubjectNode::AttachedPermanentController {
        let failure = cursor.fail("a player subject of a temporary rule");
        cursor.rollback(checkpoint);
        return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
    }
    if reading.subject == StaticPlayerSubjectNode::You
        && !reading.possessive
        && ["have", "no", "maximum", "hand", "size"]
            .iter()
            .enumerate()
            .all(|(offset, word)| word_at(cursor, offset, word))
    {
        cursor.enter("temporary-rule.no-maximum-hand-size");
        if let Err(failure) = cursor.advance_to(cursor.position() + 5) {
            cursor.rollback(checkpoint);
            return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
        }
        return close_temporary_rule(cursor, checkpoint, TemporaryRuleNode::NoMaximumHandSize);
    }
    let predicate = match super::recognize_static_player_predicate(cursor, reading) {
        Ok(predicate) => predicate,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
        }
    };
    close_temporary_rule(
        cursor,
        checkpoint,
        TemporaryRuleNode::Players {
            subject: reading.subject,
            predicate,
        },
    )
}

#[cfg(test)]
#[path = "player_permission_tests.rs"]
mod tests;
