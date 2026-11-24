use super::super::*;

/// Lower a shared object query once. Selection is a resolution decision and
/// does not add chooseTargets or any declaration-time target identity.
pub(in crate::oracle::canonical) fn lower_selected_permanents_effects(
    effect: &SelectedPermanentsEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let operand = &effect.operand;
    let (predicate, controller_filter) =
        lower_permanent_query_in_context(&operand.query, declarations)?;
    let Some(bounds) = operand.bounds else {
        // `all|each QUERY`: every matching permanent on resolution, no
        // decision. The collection reads a controller population
        // (`your opponents control`) and the `other` source exclusion.
        let mut objects = json!({"kind":"eachPermanent", "where":predicate});
        match controller_filter {
            Some(LoweredQueryController::Player(player))
            | Some(LoweredQueryController::Population(player)) => objects["player"] = player,
            None => {}
        }
        if operand.query.exclude_source {
            objects["excludeSource"] = json!(true);
        }
        return Ok(vec![match effect.action {
            SelectedPermanentAction::Untap => json!({"kind":"untapPermanents", "objects":objects}),
            SelectedPermanentAction::ReturnToOwnerHand => {
                json!({"kind":"returnToOwnersHand", "object":objects})
            }
            // The shared permanent exile writer evaluates the collection on
            // resolution (CR 608.2c), like `destroy all`.
            SelectedPermanentAction::Exile => {
                json!({"kind":"exilePermanent", "permanent":objects})
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
    match controller_filter {
        Some(LoweredQueryController::Player(controller)) => candidates["controller"] = controller,
        // A choice's candidates name one controller; a population of
        // controllers has no candidate writer yet and is refused.
        Some(LoweredQueryController::Population(_)) => {
            return Err(TargetlessLoweringError::UnsupportedCollectionController);
        }
        None => {}
    }
    // `another creature you control` (CR 109.5 source exclusion).
    if operand.query.exclude_source {
        candidates["excludeSource"] = json!(true);
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
            // A chosen set is an object collection: the card mover exiles
            // every chosen permanent, each in its owner's exile.
            SelectedPermanentAction::Exile => json!({
                "kind":"moveCards",
                "cards":decision_result(&id),
                "to":{"kind":"exile",
                    "player":{"kind":"ownerOf","object":{"kind":"candidate"}},
                    "faceDown":false},
            }),
        },
    ])
}
