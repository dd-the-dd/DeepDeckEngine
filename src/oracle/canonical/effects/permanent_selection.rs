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
    let Some(bounds) = &operand.bounds else {
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
            // `tap all QUERY`: the established collection tap (CR 611.2c).
            SelectedPermanentAction::Tap => json!({"kind":"tapPermanent", "permanent":objects}),
            SelectedPermanentAction::ReturnToOwnerHand => {
                json!({"kind":"returnToOwnersHand", "object":objects})
            }
            // The shared permanent exile writer evaluates the collection on
            // resolution (CR 608.2c), like `destroy all`. `until SOURCE
            // leaves the battlefield` links each exiled object to the source
            // (CR 610.3).
            SelectedPermanentAction::Exile => {
                let mut exile = json!({"kind":"exilePermanent", "permanent":objects});
                if let Some(ZoneChangeUntilNode::SourceLeavesBattlefield) = effect.until {
                    exile["until"] = json!({"kind":"sourceLeavesBattlefield"});
                }
                exile
            }
        }]);
    };
    // A chosen set is moved by the card mover, which links no exiled card
    // to its source: a timed exile of a choice stays unsupported.
    if effect.until.is_some() {
        return Err(TargetlessLoweringError::UnsupportedEffect);
    }
    let minimum = lower_targetless_numeric_expression(&bounds.minimum, declarations)?;
    let maximum = match &bounds.maximum {
        TargetCountBound::Amount(amount) => {
            lower_targetless_numeric_expression(amount, declarations)?
        }
        // `any number of` / `one or more` (no upper bound): every permanent
        // on the battlefield bounds the choice from above, a superset of
        // the candidates that never limits it below what is allowed; the
        // runtime clamps it to the candidates (the same population bound as
        // an unbounded target declaration).
        TargetCountBound::Unbounded => json!({"kind":"countPermanents", "allPlayers":true}),
    };
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
            // The chosen set is tapped; the tap action records the permanents
            // it actually tapped when a later `tapped this way` reads them.
            SelectedPermanentAction::Tap => {
                json!({"kind":"tapPermanent", "permanent":decision_result(&id)})
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
