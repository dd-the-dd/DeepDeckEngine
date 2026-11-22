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

#[cfg(test)]
#[path = "permanent_operand_tests.rs"]
mod tests;
