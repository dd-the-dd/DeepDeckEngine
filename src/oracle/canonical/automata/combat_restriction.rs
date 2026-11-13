use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, GrantedKeyword, TemporaryKeywordGrantNode,
    TemporaryKeywordGrantObjectsNode, recognize_effect_duration, recognize_target_list,
};

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
                cursor.enter("effect.combat-restriction.kind");
                restriction = Some(
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
                );
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
                    duration: cursor.require_value(duration, "a temporary duration")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "combat_restriction_tests.rs"]
mod tests;
