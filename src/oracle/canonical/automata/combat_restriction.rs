use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, GrantedKeyword, ObjectQueryNode, StaticConditionContext,
    StaticConditionNode, TemporaryKeywordGrantNode, TemporaryKeywordGrantObjectsNode,
    recognize_effect_duration, recognize_object_query, recognize_source_reference,
    recognize_static_condition, recognize_target_list,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemporaryCombatRestrictionState {
    Restriction,
    Duration,
    Complete,
}

pub(in crate::oracle::canonical) enum TemporaryCombatRestrictionParseAttempt {
    Parsed(TemporaryKeywordGrantNode),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

/// Transactionally recognizes a targeted temporary combat restriction. The
/// target grammar is tentative until `can't` is followed by a combat action.
/// From that action on, failures belong to this family and remain visible to
/// the parent sequence.
pub(in crate::oracle::canonical) fn try_recognize_temporary_combat_restriction(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryCombatRestrictionParseAttempt {
    let checkpoint = cursor.checkpoint();
    cursor.enter("effect.combat-restriction.targets");
    let targets = match recognize_target_list(cursor) {
        Ok(targets) => targets,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return TemporaryCombatRestrictionParseAttempt::RejectedCandidate(failure);
        }
    };

    cursor.enter("effect.combat-restriction.prohibition");
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t"))
    {
        let failure = cursor.fail("the combat prohibition `can't`");
        cursor.rollback(checkpoint);
        return TemporaryCombatRestrictionParseAttempt::RejectedCandidate(failure);
    }
    cursor.enter("effect.combat-restriction.kind");
    let combat_action_starts = cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("attack") || raw.eq_ignore_ascii_case("block"))
        || (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("be"))
            && cursor
                .peek_ahead(2)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("blocked")));
    if !combat_action_starts {
        let failure = cursor.fail("`attack`, `block`, or `be blocked`");
        cursor.rollback(checkpoint);
        return TemporaryCombatRestrictionParseAttempt::RejectedCandidate(failure);
    }
    if let Err(failure) = cursor.advance_to(cursor.position() + 1) {
        return TemporaryCombatRestrictionParseAttempt::FailedCommitted(failure);
    }

    match recognize_committed_tail(cursor, targets) {
        Ok(node) => TemporaryCombatRestrictionParseAttempt::Parsed(node),
        Err(failure) => TemporaryCombatRestrictionParseAttempt::FailedCommitted(failure),
    }
}

/// Shared combat-action child. Parent owns source/targets, prohibition and duration.
pub(in crate::oracle::canonical) fn recognize_combat_restriction_kind(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<GrantedKeyword> {
    cursor.enter("effect.combat-restriction.kind");
    Ok(
        match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("attack") => {
                cursor.consume("attack")?;
                GrantedKeyword::CantAttack
            }
            Some("block") => {
                cursor.consume("block")?;
                GrantedKeyword::CantBlock
            }
            Some("be") => {
                cursor.consume("be")?;
                cursor.enter("effect.combat-restriction.passive-action");
                cursor.consume("blocked")?;
                GrantedKeyword::CantBeBlocked
            }
            _ => return Err(cursor.fail("`attack`, `block`, or `be blocked`")),
        },
    )
}

fn recognize_committed_tail(
    cursor: &mut OracleTokenCursor<'_>,
    targets: super::TargetListNode,
) -> MachineResult<TemporaryKeywordGrantNode> {
    let mut state = TemporaryCombatRestrictionState::Restriction;
    let mut restriction = None;
    let mut duration: Option<EffectDurationNode> = None;
    loop {
        state = match state {
            TemporaryCombatRestrictionState::Restriction => {
                restriction = Some(recognize_combat_restriction_kind(cursor)?);
                TemporaryCombatRestrictionState::Duration
            }
            TemporaryCombatRestrictionState::Duration => {
                cursor.enter("effect.combat-restriction.duration");
                duration = Some(recognize_effect_duration(cursor)?);
                TemporaryCombatRestrictionState::Complete
            }
            TemporaryCombatRestrictionState::Complete => {
                cursor.enter("effect.combat-restriction.complete");
                return Ok(TemporaryKeywordGrantNode {
                    objects: TemporaryKeywordGrantObjectsNode::Targets(targets),
                    keywords: vec![
                        cursor.require_value(restriction, "a populated combat restriction")?,
                    ],
                    abilities: Vec::new(),
                    duration: cursor.require_value(duration, "a temporary duration")?,
                });
            }
        };
    }
}

/// One combat restriction or requirement that a source permanent places on
/// itself (CR 508.1c attack restrictions and requirements, CR 509.1b block
/// restrictions). Blocker and attacker qualifications are the shared object
/// query, never a recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceCombatClause {
    /// `can't attack`, `can't block`, `can't be blocked`.
    Prohibit(GrantedKeyword),
    /// `can't be blocked by more than one creature`.
    BlockedByAtMostOneCreature,
    /// `can't be blocked by QUERY`: no creature matching QUERY may block it.
    CantBeBlockedBy(ObjectQueryNode),
    /// `can't be blocked except by QUERY`: only creatures matching QUERY may block it.
    CantBeBlockedExceptBy(ObjectQueryNode),
    /// `can't block QUERY`: the source may not block attackers matching QUERY.
    CantBlockObjects(ObjectQueryNode),
    /// `attacks each combat if able`.
    AttacksEachCombat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCombatRestrictionNode {
    pub clauses: Vec<SourceCombatClause>,
    /// `as long as CONDITION` (CR 611.3a) gates every clause of the ability.
    pub condition: Option<StaticConditionNode>,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn is_prohibition_word(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can\u{2019}t")
}

fn as_long_as_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "as") && word_at(cursor, 1, "long") && word_at(cursor, 2, "as")
}

/// Static source frame:
///
/// ```text
/// Restriction := ["as long as" Condition ","] Source Clause ("and" Clause)*
///                ["as long as" Condition] "."
/// Clause      := "attacks each combat if able" | "can't" Prohibition
/// Prohibition := "attack" ["or" "block"] | "block" [Query]
///              | "be blocked" [("by" ("more than one creature" | Query))
///                              | ("except by" Query)]
/// ```
///
/// `committed` becomes true once a combat restriction word has been read, so
/// that the parent can surface this family's failure instead of a generic one.
pub(in crate::oracle::canonical) fn recognize_source_combat_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    committed: &mut bool,
) -> MachineResult<SourceCombatRestrictionNode> {
    let mut condition = None;
    if as_long_as_starts(cursor) {
        cursor.enter("static.combat-restriction.leading-condition");
        cursor.consume_phrase(&["as", "long", "as"])?;
        *committed = true;
        condition = Some(recognize_static_condition(
            cursor,
            StaticConditionContext {
                source_subject: false,
                name_links,
                name_failure,
            },
        )?);
        cursor.enter("static.combat-restriction.leading-condition.separator");
        cursor.consume(",")?;
    }
    cursor.enter("static.combat-restriction.source");
    recognize_source_reference(cursor, name_links, name_failure)?;
    let mut clauses = Vec::new();
    loop {
        recognize_source_combat_clause(cursor, committed, &mut clauses)?;
        cursor.enter("static.combat-restriction.connector");
        if word_at(cursor, 0, "and") {
            cursor.consume("and")?;
            continue;
        }
        break;
    }
    if as_long_as_starts(cursor) {
        cursor.require(
            condition.is_none(),
            "one `as long as` condition for the combat restriction",
        )?;
        cursor.enter("static.combat-restriction.trailing-condition");
        cursor.consume_phrase(&["as", "long", "as"])?;
        condition = Some(recognize_static_condition(
            cursor,
            StaticConditionContext {
                source_subject: true,
                name_links,
                name_failure,
            },
        )?);
    }
    if cursor.peek() == Some(".") {
        cursor.consume(".")?;
    }
    cursor.enter("static.combat-restriction.complete");
    cursor.finish()?;
    Ok(SourceCombatRestrictionNode { clauses, condition })
}

fn recognize_source_combat_clause(
    cursor: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
    clauses: &mut Vec<SourceCombatClause>,
) -> MachineResult<()> {
    cursor.enter("static.combat-restriction.clause");
    if word_at(cursor, 0, "attacks") {
        cursor.consume("attacks")?;
        *committed = true;
        cursor.enter("static.combat-restriction.requirement");
        cursor.consume_phrase(&["each", "combat", "if", "able"])?;
        clauses.push(SourceCombatClause::AttacksEachCombat);
        return Ok(());
    }
    cursor.enter("static.combat-restriction.prohibition");
    let word = cursor
        .peek()
        .filter(|raw| is_prohibition_word(raw))
        .ok_or_else(|| cursor.fail("the prohibition `can't` or `attacks each combat if able`"))?
        .to_owned();
    cursor.consume(&word)?;
    cursor.enter("static.combat-restriction.kind");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("attack") => {
            cursor.consume("attack")?;
            *committed = true;
            clauses.push(SourceCombatClause::Prohibit(GrantedKeyword::CantAttack));
            if word_at(cursor, 0, "or") && word_at(cursor, 1, "block") {
                cursor.consume_phrase(&["or", "block"])?;
                clauses.push(SourceCombatClause::Prohibit(GrantedKeyword::CantBlock));
            }
        }
        Some("block") => {
            cursor.consume("block")?;
            *committed = true;
            let ends_clause = cursor.peek().is_none_or(|raw| raw == ".")
                || word_at(cursor, 0, "and")
                || as_long_as_starts(cursor);
            if ends_clause {
                clauses.push(SourceCombatClause::Prohibit(GrantedKeyword::CantBlock));
            } else {
                cursor.enter("static.combat-restriction.blocked-attackers");
                clauses.push(SourceCombatClause::CantBlockObjects(
                    recognize_object_query(cursor)?,
                ));
            }
        }
        Some("be") => {
            cursor.consume("be")?;
            cursor.enter("static.combat-restriction.passive-action");
            cursor.consume("blocked")?;
            *committed = true;
            cursor.enter("static.combat-restriction.blockers");
            if word_at(cursor, 0, "by") {
                cursor.consume("by")?;
                if word_at(cursor, 0, "more") {
                    cursor.consume_phrase(&["more", "than", "one", "creature"])?;
                    clauses.push(SourceCombatClause::BlockedByAtMostOneCreature);
                } else {
                    clauses.push(SourceCombatClause::CantBeBlockedBy(recognize_object_query(
                        cursor,
                    )?));
                }
            } else if word_at(cursor, 0, "except") {
                cursor.consume_phrase(&["except", "by"])?;
                clauses.push(SourceCombatClause::CantBeBlockedExceptBy(
                    recognize_object_query(cursor)?,
                ));
            } else {
                clauses.push(SourceCombatClause::Prohibit(GrantedKeyword::CantBeBlocked));
            }
        }
        _ => return Err(cursor.fail("`attack`, `block`, or `be blocked`")),
    }
    Ok(())
}

#[cfg(test)]
#[path = "combat_restriction_tests.rs"]
mod tests;
