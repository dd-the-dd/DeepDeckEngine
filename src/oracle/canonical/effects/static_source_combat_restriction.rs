use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticSourceCombatRestrictionAttempt {
    NotClaimed,
    /// A combat restriction word was read and the rest did not fit. Later
    /// readers (temporary, conditional, unsupported) are tried before this
    /// failure is surfaced.
    Rejected(MachineFailure),
    Parsed(CanonicalRuleDraft),
}

/// Full-envelope static parent for restrictions a source places on itself
/// (CR 508.1c, 509.1b). Temporary and multi-sentence forms remain available to
/// their own parents; none is made unconditional or partially accepted.
pub(in crate::oracle::canonical) fn try_parse_static_source_combat_restriction(
    input: &AbilityInput<'_>,
) -> StaticSourceCombatRestrictionAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticSourceCombatRestrictionAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-source-combat-restriction", input.tokens);
    let mut committed = false;
    // A condition of the restriction may name the defending player of the
    // combat it is checked for (`can't attack unless defending player
    // controls an Island`, CR 508.1c, 509.1b).
    let parsed = with_combat_defending_player(&mut cursor, |cursor| {
        recognize_source_combat_restriction(
            cursor,
            input.name_links,
            input.name_resolution_failure,
            &mut committed,
        )
    })
    .and_then(|node| {
        // A condition about the defending player is checked per combat by
        // the restriction itself, never as a standing gate of the ability.
        if let Some(condition) = node
            .condition
            .as_ref()
            .filter(|condition| condition_reads_defending_player(condition))
        {
            let condition = lower_condition(condition);
            let modifiers = node
                .clauses
                .iter()
                .map(|clause| {
                    lower_defending_player_combat_clause(&cursor, &self_ref(), clause, &condition)
                })
                .collect::<MachineResult<Vec<_>>>()?;
            return Ok((modifiers, None));
        }
        let modifiers = node
            .clauses
            .iter()
            .map(|clause| lower_static_combat_clause(&cursor, &self_ref(), clause))
            .collect::<MachineResult<Vec<_>>>()?;
        Ok((modifiers, node.condition))
    });
    let (modifiers, condition) = match parsed {
        Ok(parsed) => parsed,
        Err(failure) if committed => {
            return StaticSourceCombatRestrictionAttempt::Rejected(failure);
        }
        Err(_) => return StaticSourceCombatRestrictionAttempt::NotClaimed,
    };
    let mut rule = json!({
        "kind":"staticAbility", "source":self_ref(),
        "activeWhile":active_while_battlefield(),
        "modifiers":modifiers,
    });
    if let Some(condition) = &condition {
        rule["condition"] = lower_condition(condition);
    }
    StaticSourceCombatRestrictionAttempt::Parsed(draft(
        rule,
        &[
            "Resolve the permanent source through the shared source-reference automaton",
            "Read each independent combat clause; blocker and attacker qualifications use the shared object query",
            "Lower to the engine's internal restriction markers and blockRestriction pairs, not defender or a fabricated Magic keyword",
            "Gate the whole static ability with the shared `as long as` condition",
        ],
    ))
}

/// Lowers one shared combat clause on the objects of its static subject. The
/// same clause lowers identically whether its subject is the source, an
/// attachment or a battlefield collection.
pub(in crate::oracle::canonical) fn lower_static_combat_clause(
    cursor: &OracleTokenCursor<'_>,
    objects: &Value,
    clause: &CombatClause,
) -> MachineResult<Value> {
    let marker = |keyword: GrantedKeyword| {
        json!({
            "kind":"grantKeyword", "objects":objects.clone(),
            "keyword":keyword.canonical_name(),
        })
    };
    Ok(match clause {
        CombatClause::Prohibit(keyword) => marker(*keyword),
        CombatClause::BlockedByAtMostOneCreature => marker(GrantedKeyword::MaxOneBlocker),
        // The engine's self-requirement modifier is the same marker observed
        // on its own source; other subjects receive the granted marker.
        CombatClause::AttacksEachCombat if *objects == self_ref() => json!({
            "kind":"attackEachCombatIfAble", "object":self_ref(),
        }),
        CombatClause::AttacksEachCombat => marker(GrantedKeyword::AttackEachCombatIfAble),
        CombatClause::BlocksEachCombat => marker(GrantedKeyword::BlockEachCombatIfAble),
        CombatClause::MustBeBlocked => marker(GrantedKeyword::MustBeBlockedIfAble),
        CombatClause::CantBeBlockedBy(query) => json!({
            "kind":"blockRestriction", "attackers":objects.clone(),
            "blockers":lower_combat_object_selector(cursor, query, None)?,
        }),
        CombatClause::CantBeBlockedExceptBy(query) => json!({
            "kind":"blockRestriction", "attackers":objects.clone(),
            "blockers":lower_combat_object_selector(
                cursor,
                query,
                Some(CombatDomainComplement::Creatures),
            )?,
        }),
        CombatClause::CantBlockObjects(query) => json!({
            "kind":"blockRestriction",
            "attackers":lower_combat_object_selector(cursor, query, None)?,
            "blockers":objects.clone(),
        }),
        CombatClause::BlockOnly(query) => json!({
            "kind":"blockRestriction",
            "attackers":lower_combat_object_selector(
                cursor,
                query,
                Some(CombatDomainComplement::Creatures),
            )?,
            "blockers":objects.clone(),
        }),
        CombatClause::CantAttackController { planeswalkers } => json!({
            "kind":"cantAttackSourceController", "objects":objects.clone(),
            "defenders": if *planeswalkers { "playerAndPlaneswalkers" } else { "player" },
        }),
    })
}

/// One combat clause whose application depends on the defending player of
/// the combat (CR 506.2): `can't attack` toward a defending player that
/// satisfies `condition` (CR 508.1c), `can't be blocked [by QUERY]` in a
/// combat whose defending player satisfies it (CR 509.1b). The runtime binds
/// the defending player at each check; any other clause is refused.
fn lower_defending_player_combat_clause(
    cursor: &OracleTokenCursor<'_>,
    objects: &Value,
    clause: &CombatClause,
    condition: &Value,
) -> MachineResult<Value> {
    let restriction = |restriction: &str| {
        json!({
            "kind":"defendingPlayerCombatRestriction", "objects":objects.clone(),
            "restriction":restriction, "condition":condition.clone(),
        })
    };
    match clause {
        CombatClause::Prohibit(GrantedKeyword::CantAttack) => Ok(restriction("cantAttack")),
        CombatClause::Prohibit(GrantedKeyword::CantBeBlocked) => Ok(restriction("cantBeBlocked")),
        CombatClause::CantBeBlockedBy(query) => {
            let mut lowered = restriction("cantBeBlockedBy");
            lowered["blockers"] = lower_combat_object_selector(cursor, query, None)?;
            Ok(lowered)
        }
        _ => Err(cursor.fail(
            "`can't attack`, `can't be blocked` or `can't be blocked by QUERY` for a condition about the defending player",
        )),
    }
}

#[derive(Clone, Copy)]
enum CombatDomainComplement {
    Creatures,
}

/// One shared object query as the engine's permanent selector. A complement is
/// taken inside the creature domain, so `except by Walls` prohibits every
/// creature that is not a Wall (CR 509.1b).
fn lower_combat_object_selector(
    cursor: &OracleTokenCursor<'_>,
    query: &ObjectQueryNode,
    complement: Option<CombatDomainComplement>,
) -> MachineResult<Value> {
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent),
        "a combat object query anchored in the battlefield-permanent domain",
    )?;
    let selected = match complement {
        None => Some(lower_permanent_object_predicate(&query.predicate)),
        Some(CombatDomainComplement::Creatures) => complement_within_creatures(&query.predicate),
    }
    .filter(|lowered| !lowered.is_null());
    let mut selector = json!({
        "kind":"permanents",
        "where":cursor.require_value(
            selected,
            "a combat object query with at least one qualifying characteristic",
        )?,
    });
    if let Some(scope) = query.controller {
        selector["controller"] = match scope {
            // Every player other than the controller is an opponent, the
            // relation the engine evaluates for blockers and attackers.
            ObjectControllerScope::AbilityController => controller(),
            ObjectControllerScope::NotAbilityController
            | ObjectControllerScope::OpponentOfAbilityController => {
                json!({ "kind":"opponentsOf", "player":controller() })
            }
            // A static ability has no triggering event and declares no
            // player target.
            ObjectControllerScope::EventPlayer | ObjectControllerScope::Player(_) => {
                return Err(cursor.fail("a static controller scope without an event player"));
            }
        };
    }
    Ok(selector)
}

#[cfg(test)]
#[path = "static_source_combat_restriction_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "combat_clause_family_tests.rs"]
mod family_tests;
