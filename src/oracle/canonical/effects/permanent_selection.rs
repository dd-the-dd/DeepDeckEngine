use super::super::*;

/// Lower a shared object query once. Selection is a resolution decision and
/// does not add chooseTargets or any declaration-time target identity.
pub(in crate::oracle::canonical) fn lower_untap_permanents_effects(
    effect: &UntapPermanentsEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let operand = &effect.operand;
    let (predicate, controller_filter) = lower_fixed_controller_permanent_query(&operand.query)?;
    let minimum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(operand.bounds.minimum),
        declarations,
    )?;
    let maximum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(operand.bounds.maximum),
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
        json!({"kind":"untapPermanents", "objects":decision_result(&id)}),
    ])
}
