use super::super::*;

/// Preserve a source reference exactly, or request the collection once at
/// resolution. A quantifier never creates a target declaration or a prompt.
pub(in crate::oracle::canonical) fn lower_nontargeted_permanent_operand(
    operand: &PermanentOperandNode,
) -> Result<Value, TargetlessLoweringError> {
    match operand {
        PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_) => {
            Err(TargetlessLoweringError::RequiresTarget)
        }
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf) => Ok(self_ref()),
        PermanentOperandNode::Collection {
            objects,
            exclude_source,
        } => {
            let (predicate, controller_filter) = lower_fixed_controller_permanent_query(objects)?;
            let mut object = json!({"kind":"eachPermanent", "where":predicate});
            if let Some(player) = controller_filter {
                object["player"] = player;
            }
            if *exclude_source {
                object["excludeSource"] = json!(true);
            }
            Ok(object)
        }
    }
}

/// Shared predicate/controller capability for non-targeted permanent queries.
pub(in crate::oracle::canonical) fn lower_fixed_controller_permanent_query(
    query: &ObjectQueryNode,
) -> Result<(Value, Option<Value>), TargetlessLoweringError> {
    if object_predicate_uses_x(&query.predicate) {
        return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
    }
    let controller_filter = match query.controller {
        None => None,
        Some(ObjectControllerScope::AbilityController) => Some(controller()),
        Some(ObjectControllerScope::OpponentOfAbilityController)
        | Some(ObjectControllerScope::NotAbilityController)
        | Some(ObjectControllerScope::EventPlayer) => {
            return Err(TargetlessLoweringError::UnsupportedCollectionController);
        }
    };
    Ok((
        lower_permanent_object_predicate(&query.predicate),
        controller_filter,
    ))
}

/// The player relation of a lowered non-target permanent query: one player
/// (a choice's candidates can only name one controller) or a population
/// (a universal collection reads every member's controller).
#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) enum LoweredQueryController {
    Player(Value),
    Population(Value),
}

/// The shared non-target permanent query with every controller relation
/// and with numeric comparisons validated in the declaration context:
/// `with toughness X or less` reads X only when the face declares it or a
/// `where X is` definition of the same sentence supplies it (CR 107.3).
pub(in crate::oracle::canonical) fn lower_permanent_query_in_context(
    query: &ObjectQueryNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<(Value, Option<LoweredQueryController>), TargetlessLoweringError> {
    validate_object_predicate_numeric_expressions(&query.predicate, declarations)?;
    let controller = query.controller.map(|scope| match scope {
        ObjectControllerScope::AbilityController | ObjectControllerScope::EventPlayer => {
            LoweredQueryController::Player(lower_object_controller_scope(scope))
        }
        ObjectControllerScope::OpponentOfAbilityController
        | ObjectControllerScope::NotAbilityController => {
            LoweredQueryController::Population(lower_object_controller_scope(scope))
        }
    });
    Ok((
        lower_permanent_object_predicate(&query.predicate),
        controller,
    ))
}

/// One declared-count bound as a numeric IR value. A literal stays a
/// literal; `X` reads the face's X decision; other amounts use the shared
/// numeric lowering and fail when it cannot represent them.
pub(in crate::oracle::canonical) fn lower_target_count_amount(
    amount: &NumericExpression,
) -> Option<Value> {
    match amount {
        NumericExpression::Atom(atom) => Some(lower_target_cardinality_bound(*atom)),
        _ => lower_numeric_expression(amount),
    }
}

/// `any number of target ...` (CR 601.2c): no bound below the population
/// of the candidates' domain. The maximum is the declaration-time count of
/// every object of that domain, a superset of the legal candidates, so it
/// never limits the declaration below what the rules allow. Only domains
/// with a declaration-time population count are admitted.
pub(in crate::oracle::canonical) fn lower_unbounded_target_maximum(
    candidates: &Value,
) -> Option<Value> {
    match candidates["kind"].as_str()? {
        "permanents" => Some(json!({"kind":"countPermanents", "allPlayers":true})),
        // Every player in the game bounds any number of target players.
        "players" => Some(json!({"kind":"countPlayers"})),
        "cards"
            if matches!(
                candidates["zone"]["kind"].as_str(),
                Some("graveyard" | "anyGraveyard")
            ) =>
        {
            Some(json!({"kind":"countCards",
                "zone":{"kind":"graveyard", "player":controller()}, "allPlayers":true}))
        }
        _ => None,
    }
}

/// Overwrites the literal bounds of a lowered `chooseTargets` decision with
/// the slot's typed count. A slot without a count keeps its literal IR.
pub(in crate::oracle::canonical) fn apply_target_count_to_decision(
    decision: &mut Value,
    count: &TargetCount,
) -> Option<()> {
    decision["minimum"] = lower_target_count_amount(&count.minimum)?;
    decision["maximum"] = match &count.maximum {
        TargetCountBound::Amount(amount) => lower_target_count_amount(amount)?,
        TargetCountBound::Unbounded => lower_unbounded_target_maximum(&decision["candidates"])?,
    };
    Some(())
}

#[cfg(test)]
#[path = "permanent_operand_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "verb_operand_lowering_tests.rs"]
mod verb_operand_lowering_tests;
