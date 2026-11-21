//! Lower a temporary allowance only within the existing primitive contract.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_temporary_player_permission(
    node: &TemporaryPlayerPermissionNode,
) -> Result<Value, TargetlessLoweringError> {
    if node.duration != EffectDurationNode::UntilEndOfCurrentTurn {
        return Err(TargetlessLoweringError::UnsupportedPermissionLifetime);
    }
    match &node.modifier {
        PlayerModifierNode::AdditionalLandPlay { player, count } => {
            if *player != LocalPlayerReference::Controller {
                return Err(TargetlessLoweringError::RequiresTarget);
            }
            let NumericExpression::Atom(NumericAtom::Integer(count)) = count else {
                // Runtime currently reads an i32 literal, not a numeric program.
                return Err(TargetlessLoweringError::UnsupportedPermissionAmount);
            };
            if *count <= 0 || i32::try_from(*count).is_err() {
                return Err(TargetlessLoweringError::UnsupportedPermissionAmount);
            }
            Ok(json!({
                "kind":"grantAdditionalLandPlays", "player":controller(),
                "count":integer(*count), "duration":{"kind":"untilEndOfCurrentTurn"},
            }))
        }
        PlayerModifierNode::NoMaximumHandSize { .. } => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

#[cfg(test)]
#[path = "player_permission_tests.rs"]
mod tests;
