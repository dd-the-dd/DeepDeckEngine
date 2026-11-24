use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    BooleanConditionNode, EffectDurationNode, GrantedKeyword, ObjectQueryNode,
    StaticConditionContext, StaticConditionNode, TemporaryKeywordGrantNode,
    TemporaryKeywordGrantObjectsNode, recognize_effect_duration, recognize_object_query,
    recognize_source_reference, recognize_static_condition, recognize_target_list,
    recognize_trailing_or_supplied_duration,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

pub(in crate::oracle::canonical) enum TemporaryCombatRestrictionParseAttempt {
    Parsed(TemporaryKeywordGrantNode),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

/// Transactionally recognizes a targeted temporary combat restriction. The
/// target grammar is tentative until a combat clause begins. From the
/// clause's action word on, failures belong to this family and remain visible
/// to the parent sequence.
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
    cursor.enter("effect.combat-restriction.kind");
    if !combat_clause_starts(cursor) {
        let failure = cursor.fail("a combat restriction or requirement clause");
        cursor.rollback(checkpoint);
        return TemporaryCombatRestrictionParseAttempt::RejectedCandidate(failure);
    }
    match recognize_temporary_combat_clause(cursor, None) {
        Ok((keywords, duration)) => {
            TemporaryCombatRestrictionParseAttempt::Parsed(TemporaryKeywordGrantNode {
                objects: TemporaryKeywordGrantObjectsNode::Targets(targets),
                keywords,
                abilities: Vec::new(),
                duration,
            })
        }
        Err(failure) => TemporaryCombatRestrictionParseAttempt::FailedCommitted(failure),
    }
}

/// Temporary effect frame of the shared combat clause (CR 611.2a):
///
/// ```text
/// Temporary := Clause [Duration]      -- one duration, trailing, inner, or preposed
/// ```
///
/// Each clause becomes the restriction marker of the affected objects for
/// the duration. A clause without a per-object marker (`by QUERY`, `except
/// by`, `only`, `can't attack you`) fails here explicitly.
pub(in crate::oracle::canonical) fn recognize_temporary_combat_clause(
    cursor: &mut OracleTokenCursor<'_>,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<(Vec<GrantedKeyword>, EffectDurationNode)> {
    let mut committed = false;
    let mut clauses = Vec::new();
    let mut inner_duration = None;
    recognize_combat_clause(
        cursor,
        CombatClauseContext::Temporary,
        &mut committed,
        &mut clauses,
        &mut inner_duration,
    )?;
    cursor.enter("effect.combat-restriction.duration");
    let duration = match inner_duration {
        Some(inner) => {
            cursor.require(
                preposed_duration.is_none(),
                "no inner duration after a sentence-level preposed duration",
            )?;
            inner
        }
        None => recognize_trailing_or_supplied_duration(cursor, preposed_duration)?,
    };
    cursor.enter("effect.combat-restriction.marker");
    let keywords = clauses
        .iter()
        .map(CombatClause::temporary_marker)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            cursor.fail(
                "a temporary combat clause representable as a restriction marker on the affected objects",
            )
        })?;
    Ok((keywords, duration))
}

// One combat restriction or requirement placed on the objects of an
/// enclosing subject (CR 508.1c-d attack restrictions and requirements,
/// CR 509.1b-c block restrictions and requirements). The clause carries no
/// subject: the static source frame, the static continuous frame and the
/// temporary effect frame each supply their own. Blocker and attacker
/// qualifications are the shared object query, never a recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CombatClause {
    /// `can't attack`, `can't block`, `can't be blocked`.
    Prohibit(GrantedKeyword),
    /// `can't be blocked by more than one creature`.
    BlockedByAtMostOneCreature,
    /// `can't be blocked by QUERY`: no creature matching QUERY may block it.
    CantBeBlockedBy(ObjectQueryNode),
    /// `can't be blocked except by QUERY`: only creatures matching QUERY may block it.
    CantBeBlockedExceptBy(ObjectQueryNode),
    /// `can't block QUERY`: the objects may not block attackers matching QUERY.
    CantBlockObjects(ObjectQueryNode),
    /// `can block only QUERY`: the objects may block only attackers matching QUERY.
    BlockOnly(ObjectQueryNode),
    /// `can't attack you [or planeswalkers you control]` (CR 508.1c): the
    /// ability controller, and optionally its planeswalkers, are not legal
    /// defenders for the objects.
    CantAttackController { planeswalkers: bool },
    /// `attack(s) each combat if able` / `attack(s) this turn if able`.
    AttacksEachCombat,
    /// `block(s) each combat if able` / `block(s) this turn if able`.
    BlocksEachCombat,
    /// `must be blocked [this turn] if able` (CR 509.1c).
    MustBeBlocked,
}

impl CombatClause {
    /// The restriction marker a temporary effect places on each affected
    /// object for its duration. Pair restrictions (`by QUERY`, `except by`,
    /// `only`) and defender restrictions have no per-object marker.
    pub(in crate::oracle::canonical) fn temporary_marker(&self) -> Option<GrantedKeyword> {
        Some(match self {
            Self::Prohibit(keyword) => *keyword,
            Self::BlockedByAtMostOneCreature => GrantedKeyword::MaxOneBlocker,
            Self::AttacksEachCombat => GrantedKeyword::AttackEachCombatIfAble,
            Self::BlocksEachCombat => GrantedKeyword::BlockEachCombatIfAble,
            Self::MustBeBlocked => GrantedKeyword::MustBeBlockedIfAble,
            Self::CantBeBlockedBy(_)
            | Self::CantBeBlockedExceptBy(_)
            | Self::CantBlockObjects(_)
            | Self::BlockOnly(_)
            | Self::CantAttackController { .. } => return None,
        })
    }
}

/// Where a combat clause occurs. A static clause states a standing
/// requirement (`each combat`); a temporary clause may instead carry its own
/// duration between the action and `if able` (`attacks this turn if able`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CombatClauseContext {
    Static,
    Temporary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCombatRestrictionNode {
    pub clauses: Vec<CombatClause>,
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

/// The word that introduces a trailing condition of a standing restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrailingCondition {
    AsLongAs,
    If,
    Unless,
}

/// `as long as`, `unless`, or `if` not followed by `able` (a requirement's
/// own `if able` belongs to the clause).
fn trailing_condition_introducer(cursor: &OracleTokenCursor<'_>) -> Option<TrailingCondition> {
    if as_long_as_starts(cursor) {
        Some(TrailingCondition::AsLongAs)
    } else if word_at(cursor, 0, "unless") {
        Some(TrailingCondition::Unless)
    } else if word_at(cursor, 0, "if") && !word_at(cursor, 1, "able") {
        Some(TrailingCondition::If)
    } else {
        None
    }
}

fn duration_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    word_at(cursor, offset, "until")
        || (word_at(cursor, offset, "this")
            && (word_at(cursor, offset + 1, "turn") || word_at(cursor, offset + 1, "combat")))
}

/// True when the tokens at the cursor begin a combat clause: the lexical
/// envelope a parent checks before it hands the clause to this child. The
/// clause itself commits only after its combat action word.
pub(in crate::oracle::canonical) fn combat_clause_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let action_follows = |offset: usize| {
        word_at(cursor, offset, "attack")
            || word_at(cursor, offset, "block")
            || (word_at(cursor, offset, "be") && word_at(cursor, offset + 1, "blocked"))
    };
    let requirement_follows =
        |offset: usize| word_at(cursor, offset, "each") || duration_starts_at(cursor, offset);
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some(word) if is_prohibition_word(word) => action_follows(1),
        Some("can") => word_at(cursor, 1, "block") && word_at(cursor, 2, "only"),
        Some("attack" | "attacks" | "block" | "blocks") => requirement_follows(1),
        Some("must") => word_at(cursor, 1, "be") && word_at(cursor, 2, "blocked"),
        _ => false,
    }
}

/// Static source frame:
///
/// ```text
/// Restriction := ["as long as" Condition ","] Source Clause ("and" Clause)*
///                ["as long as" Condition] "."
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
        recognize_combat_clause(
            cursor,
            CombatClauseContext::Static,
            committed,
            &mut clauses,
            &mut None,
        )?;
        cursor.enter("static.combat-restriction.connector");
        if word_at(cursor, 0, "and") {
            cursor.consume("and")?;
            continue;
        }
        break;
    }
    if let Some(introducer) = trailing_condition_introducer(cursor) {
        // `as long as C` and `if C` gate the restriction on C; `unless C`
        // gates it on not-C (CR 508.1c, 509.1b: checked as the attack or
        // block is declared, which a continuously evaluated static condition
        // observes). Each is the one shared static condition language.
        cursor.require(
            condition.is_none(),
            "one condition for the combat restriction",
        )?;
        cursor.enter("static.combat-restriction.trailing-condition");
        match introducer {
            TrailingCondition::AsLongAs => cursor.consume_phrase(&["as", "long", "as"])?,
            TrailingCondition::If => cursor.consume("if")?,
            TrailingCondition::Unless => cursor.consume("unless")?,
        }
        *committed = true;
        let read = recognize_static_condition(
            cursor,
            StaticConditionContext {
                source_subject: true,
                name_links,
                name_failure,
            },
        )?;
        condition = Some(match introducer {
            TrailingCondition::AsLongAs | TrailingCondition::If => read,
            TrailingCondition::Unless => BooleanConditionNode::Not(Box::new(read)),
        });
    }
    if cursor.peek() == Some(".") {
        cursor.consume(".")?;
    }
    cursor.enter("static.combat-restriction.complete");
    cursor.finish()?;
    Ok(SourceCombatRestrictionNode { clauses, condition })
}

/// `if able` closes a requirement (CR 508.1d, 509.1c). An attack or block
/// requirement holds `each combat`; `must be blocked` (`bare_if_able`) holds
/// whenever the object attacks. A temporary requirement may name its own
/// duration instead, returned through `duration` for the effect parent.
fn recognize_requirement_tail(
    cursor: &mut OracleTokenCursor<'_>,
    context: CombatClauseContext,
    bare_if_able: bool,
    duration: &mut Option<EffectDurationNode>,
) -> MachineResult<()> {
    cursor.enter("combat-clause.requirement");
    if bare_if_able && word_at(cursor, 0, "if") {
        // `must be blocked if able`: no recurrence word.
    } else if word_at(cursor, 0, "each") {
        cursor.consume_phrase(&["each", "combat"])?;
    } else {
        cursor.require(
            context == CombatClauseContext::Temporary,
            "`each combat if able` for a standing requirement",
        )?;
        cursor.require(
            duration.is_none(),
            "one duration for the temporary requirement",
        )?;
        cursor.enter("combat-clause.requirement.duration");
        *duration = Some(recognize_effect_duration(cursor)?);
    }
    cursor.enter("combat-clause.requirement.if-able");
    cursor.consume_phrase(&["if", "able"])
}

/// A bare `can't block` ends at a connector, a condition, a duration or the
/// sentence end; anything else is the attacker query of `can't block QUERY`.
fn bare_block_prohibition_ends(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_none_or(|raw| raw == "." || raw == ",")
        || word_at(cursor, 0, "and")
        || trailing_condition_introducer(cursor).is_some()
        || duration_starts_at(cursor, 0)
}

/// Shared combat clause child:
///
/// ```text
/// Clause      := ("attack" | "attacks") Requirement
///              | ("block" | "blocks") Requirement
///              | "must be blocked" Requirement
///              | "can block only" Query
///              | "can't" Prohibition
/// Requirement := "each combat if able" | Duration "if able"   (temporary only)
/// Prohibition := "attack" ["or" "block"] | "attack you" ["or planeswalkers you control"]
///              | "block" [Query]
///              | "be blocked" [("by" ("more than one creature" | Query))
///                              | ("except by" Query)]
/// ```
///
/// `committed` becomes true after the combat action word.
pub(in crate::oracle::canonical) fn recognize_combat_clause(
    cursor: &mut OracleTokenCursor<'_>,
    context: CombatClauseContext,
    committed: &mut bool,
    clauses: &mut Vec<CombatClause>,
    duration: &mut Option<EffectDurationNode>,
) -> MachineResult<()> {
    cursor.enter("static.combat-restriction.clause");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some(verb @ ("attack" | "attacks" | "block" | "blocks")) => {
            let attack = verb.starts_with("attack");
            let verb = verb.to_owned();
            cursor.consume(&verb)?;
            *committed = true;
            recognize_requirement_tail(cursor, context, false, duration)?;
            clauses.push(if attack {
                CombatClause::AttacksEachCombat
            } else {
                CombatClause::BlocksEachCombat
            });
            return Ok(());
        }
        Some("must") => {
            cursor.consume("must")?;
            cursor.enter("combat-clause.must-be-blocked");
            cursor.consume_phrase(&["be", "blocked"])?;
            *committed = true;
            recognize_requirement_tail(cursor, context, true, duration)?;
            clauses.push(CombatClause::MustBeBlocked);
            return Ok(());
        }
        Some("can") => {
            cursor.consume("can")?;
            cursor.enter("combat-clause.block-only");
            cursor.consume("block")?;
            *committed = true;
            cursor.consume("only")?;
            cursor.enter("combat-clause.block-only.attackers");
            clauses.push(CombatClause::BlockOnly(recognize_object_query(cursor)?));
            return Ok(());
        }
        _ => {}
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
            if word_at(cursor, 0, "you") {
                cursor.enter("combat-clause.defender");
                cursor.consume("you")?;
                let planeswalkers = word_at(cursor, 0, "or");
                if planeswalkers {
                    cursor.consume_phrase(&["or", "planeswalkers", "you", "control"])?;
                }
                clauses.push(CombatClause::CantAttackController { planeswalkers });
                return Ok(());
            }
            clauses.push(CombatClause::Prohibit(GrantedKeyword::CantAttack));
            if word_at(cursor, 0, "or") && word_at(cursor, 1, "block") {
                cursor.consume_phrase(&["or", "block"])?;
                clauses.push(CombatClause::Prohibit(GrantedKeyword::CantBlock));
            }
        }
        Some("block") => {
            cursor.consume("block")?;
            *committed = true;
            if bare_block_prohibition_ends(cursor) {
                clauses.push(CombatClause::Prohibit(GrantedKeyword::CantBlock));
            } else {
                cursor.enter("static.combat-restriction.blocked-attackers");
                clauses.push(CombatClause::CantBlockObjects(recognize_object_query(
                    cursor,
                )?));
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
                    clauses.push(CombatClause::BlockedByAtMostOneCreature);
                } else {
                    clauses.push(CombatClause::CantBeBlockedBy(recognize_object_query(
                        cursor,
                    )?));
                }
            } else if word_at(cursor, 0, "except") {
                cursor.consume_phrase(&["except", "by"])?;
                clauses.push(CombatClause::CantBeBlockedExceptBy(recognize_object_query(
                    cursor,
                )?));
            } else {
                clauses.push(CombatClause::Prohibit(GrantedKeyword::CantBeBlocked));
            }
        }
        _ => return Err(cursor.fail("`attack`, `block`, or `be blocked`")),
    }
    Ok(())
}

#[cfg(test)]
#[path = "combat_restriction_tests.rs"]
mod tests;
