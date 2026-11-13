use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims one atomic targeted battlefield zone change, optionally followed by
/// a parenthetical batch-definition reminder. Child automata own the selector
/// and reminder content after this token-shape boundary commits.
pub(in crate::oracle::canonical) fn claims_promoted_atomic_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let action = tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy")
            || token.raw.eq_ignore_ascii_case("exile")
            || token.raw.eq_ignore_ascii_case("return")
    });
    let targeted = tokens.get(1).is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("target") || token.raw.eq_ignore_ascii_case("up")
    });
    let Some(first_terminator) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    let suffix = &tokens[first_terminator + 1..];
    action
        && targeted
        && (suffix.is_empty() || suffix.first().is_some_and(|token| token.raw == "("))
}

pub(in crate::oracle::canonical) fn parse_promoted_atomic_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("atomic-targeted-zone-change", input.tokens);
    let effect = recognize_zone_change_effect(&mut cursor)?;
    let LocalEffectNode::ZoneChange(effect) = effect else {
        return Err(cursor.fail("an atomic targeted zone change"));
    };
    cursor.enter("effect.zone-change.terminator");
    cursor.consume(".")?;

    if cursor.peek().is_some_and(|raw| raw == "(") {
        let reminder = recognize_characteristic_batch_reminder(&mut cursor)?;
        cursor.enter("effect.zone-change.reminder.batch-match");
        cursor.require(
            target_contains_batch(&effect.target, reminder.batch),
            "a reminder describing the target selector's characteristic batch",
        )?;
    }
    cursor.enter("effect.zone-change.full-consumption");
    cursor.finish()?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&effect.target);
    let target = chosen_target(target_id);
    let operation = lower_targeted_zone_change_node(&mut cursor, &effect, target)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    minimum,
                    maximum,
                )],
            },
            "effects": [operation],
        }),
        &[
            "Recognize one atomic targeted battlefield zone change",
            "Resolve subtype and characteristic-batch selector predicates",
            "Validate optional batch reminder from the shared registry",
            "Lower the batch into primitive subtype predicates",
            &trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn lower_targeted_zone_change_node(
    cursor: &mut OracleTokenCursor<'_>,
    effect: &ZoneChangeEffectNode,
    target: Value,
) -> MachineResult<Value> {
    cursor.enter("lower.zone-change.regeneration-policy");
    cursor.require(
        effect.action == ZoneChangeAction::Destroy
            || effect.regeneration == ZoneChangeRegenerationPolicy::Allowed,
        "a regeneration prohibition only on a destroy action",
    )?;
    let mut operation = lower_targeted_zone_change_effect(effect.action, target);
    if matches!(
        effect.regeneration,
        ZoneChangeRegenerationPolicy::Prohibited { .. }
    ) {
        operation["cannotRegenerate"] = Value::Bool(true);
    }
    Ok(operation)
}

pub(in crate::oracle::canonical) fn lower_targeted_zone_change_effect(
    action: ZoneChangeAction,
    target: Value,
) -> Value {
    match action {
        ZoneChangeAction::Destroy => json!({
            "kind": "destroyPermanent",
            "permanent": target,
        }),
        ZoneChangeAction::Exile => json!({
            "kind": "exilePermanent",
            "permanent": target,
        }),
        ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand) => json!({
            "kind": "returnToOwnersHand",
            "object": target,
        }),
    }
}

fn target_contains_batch(target: &TargetListNode, expected: CharacteristicBatch) -> bool {
    target.slots.iter().any(|slot| match &slot.selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate_contains_batch(predicate, expected)
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => false,
    })
}

fn predicate_contains_batch(predicate: &SelectorPredicate, expected: CharacteristicBatch) -> bool {
    match predicate {
        SelectorPredicate::CharacteristicBatch(batch) => *batch == expected,
        SelectorPredicate::Not(operand) => predicate_contains_batch(operand, expected),
        SelectorPredicate::Intersection(operands) | SelectorPredicate::Union(operands) => operands
            .iter()
            .any(|operand| predicate_contains_batch(operand, expected)),
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::CardType(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::Color(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::ObjectKind(_)
        | SelectorPredicate::NumericComparison { .. } => false,
    }
}
