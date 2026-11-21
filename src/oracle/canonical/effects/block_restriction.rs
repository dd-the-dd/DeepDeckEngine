use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn without_creature_domain(predicate: &ObjectPredicate) -> Option<ObjectPredicate> {
    match predicate {
        ObjectPredicate::Intersection(operands) => {
            let mut retained = operands
                .iter()
                .filter(|operand| {
                    !matches!(operand, ObjectPredicate::CardType(ObjectCardType::Creature))
                })
                .cloned()
                .collect::<Vec<_>>();
            if retained.len() == operands.len() {
                return None;
            }
            match retained.len() {
                0 => None,
                1 => retained.pop(),
                _ => Some(ObjectPredicate::Intersection(retained)),
            }
        }
        ObjectPredicate::CardType(ObjectCardType::Creature) => None,
        _ => None,
    }
}

/// `creatures that are not QUERY`: the complement of a characteristic inside
/// the creature domain. A query that is only `creature` has no complement.
pub(in crate::oracle::canonical) fn complement_within_creatures(
    predicate: &ObjectPredicate,
) -> Option<Value> {
    if matches!(
        predicate,
        ObjectPredicate::CardType(ObjectCardType::Creature)
    ) {
        return None;
    }
    let qualifier = without_creature_domain(predicate).unwrap_or_else(|| predicate.clone());
    let lowered = lower_permanent_object_predicate(&qualifier);
    (!lowered.is_null()).then(|| {
        and(vec![
            card_type(ObjectCardType::Creature.ir_name()),
            not(lowered),
        ])
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_block_only(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("block-restriction", input.tokens);
    let restriction = recognize_block_only_restriction(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("block-restriction.lower.source");
    cursor.require(
        restriction.source == LocalObjectReference::SourceSelf,
        "the ability source as blocker",
    )?;
    cursor.enter("block-restriction.lower.selector-domain");
    cursor.require(
        restriction.allowed_attackers.controller.is_none(),
        "an attacker selector without a controller qualifier",
    )?;
    let allowed_qualifier = without_creature_domain(&restriction.allowed_attackers.predicate)
        .ok_or_else(|| {
            cursor.fail("a creature selector with at least one allowed characteristic")
        })?;
    let disallowed_attackers = and(vec![
        card_type(ObjectCardType::Creature.ir_name()),
        not(lower_permanent_object_predicate(&allowed_qualifier)),
    ]);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "blockRestriction",
                "attackers": {
                    "kind": "permanents",
                    "where": disallowed_attackers,
                },
                "blockers": self_ref(),
            }],
        }),
        &[
            "Resolve the blocker through the shared source-reference automaton",
            "Recognize the allowed attackers through the shared object-selector automaton",
            "Complement the allowed characteristic inside the creature-attacker domain",
            "Lower the prohibited pairs to the generic blockRestriction modifier",
            &trace,
        ],
    ))
}
