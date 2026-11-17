use super::super::*;

/// Lower a shared object query once. Selection is a resolution decision and
/// does not add chooseTargets or any declaration-time target identity.
pub(in crate::oracle::canonical) fn lower_selected_permanents_effects(
    effect: &SelectedPermanentsEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let operand = &effect.operand;
    let (predicate, controller_filter) = lower_fixed_controller_permanent_query(&operand.query)?;
    let Some(bounds) = operand.bounds else {
        // `all QUERY`: every matching permanent on resolution, no decision.
        let mut objects = json!({"kind":"eachPermanent", "where":predicate});
        if let Some(controller) = controller_filter {
            objects["player"] = controller;
        }
        return Ok(vec![match effect.action {
            SelectedPermanentAction::Untap => json!({"kind":"untapPermanents", "objects":objects}),
            SelectedPermanentAction::ReturnToOwnerHand => {
                json!({"kind":"returnToOwnersHand", "object":objects})
            }
        }]);
    };
    let minimum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(bounds.minimum),
        declarations,
    )?;
    let maximum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(bounds.maximum),
        declarations,
    )?;
    let id = scope.map_or_else(
        || "selectedPermanents".to_string(),
        |scope| format!("{scope}:selectedPermanents"),
    );
    let mut candidates = json!({"kind":"permanents", "where":predicate});
    if let Some(controller) = controller_filter {
        candidates["controller"] = controller;
    }
    Ok(vec![
        json!({"kind":"choosePermanents", "id":id, "player":controller(),
            "candidates":candidates, "minimum":minimum, "maximum":maximum}),
        match effect.action {
            SelectedPermanentAction::Untap => {
                json!({"kind":"untapPermanents", "objects":decision_result(&id)})
            }
            SelectedPermanentAction::ReturnToOwnerHand => {
                json!({"kind":"returnToOwnersHand", "object":decision_result(&id)})
            }
        },
    ])
}
