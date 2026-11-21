use super::super::*;

pub(in crate::oracle::canonical) const OPTIONAL_CARD_DEPLOYMENT_DECISION_ID: &str =
    "optionalCardDeployment";

fn controller_card_zone(zone: CardDeploymentSourceZone, player: Value) -> Value {
    let kind = match zone {
        CardDeploymentSourceZone::Hand => "hand",
        CardDeploymentSourceZone::Graveyard => "graveyard",
    };
    json!({ "kind": kind, "player": player })
}

/// Action-local permission: choosing zero does not skip later independent actions.
/// The source predicate is evaluated during resolution, never at declaration.
pub(in crate::oracle::canonical) fn lower_optional_card_deployment(
    deployment: &OptionalCardDeploymentEffectNode,
    declarations: TargetlessDeclarationSupport,
    id: &str,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    if deployment.player != LocalPlayerReference::Controller {
        return Err(TargetlessLoweringError::RequiresTarget);
    }
    validate_object_predicate_numeric_expressions(&deployment.filter, declarations)?;
    if !domain_filter_is_representable(&deployment.filter) {
        return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
    }
    Ok(vec![
        json!({"kind":"chooseCards","id":id,"player":controller(),"minimum":integer(0),"maximum":integer(1),
            "candidates":{"kind":"cards","zone":controller_card_zone(deployment.source,controller()),"where":lower_card_object_predicate(&deployment.filter)}}),
        json!({"kind":"moveCards","cards":decision_result(id),"to":{"kind":"battlefield","player":controller(),"tapped":deployment.tapped}}),
    ])
}
