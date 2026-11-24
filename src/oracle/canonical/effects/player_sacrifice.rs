use super::super::*;

/// The acting player's own choice of permanents to sacrifice (CR 701.21a):
/// the shared sacrifice quantity and object query become a `chooseCards`
/// over that player's permanents. Population parents wrap it in an iteration.
pub(in crate::oracle::canonical) fn lower_player_sacrifice_choice(
    action: &PlayerSacrificeActionNode,
    player: Value,
    decision: &str,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    // `a creature with the greatest power among creatures they control`:
    // the ranking of the chooser's own candidates (shared superlative).
    let (predicate, extremum) = match split_candidate_extremum(&action.object.predicate) {
        None => (action.object.predicate.clone(), None),
        Some(Some((candidates, extremum))) => (candidates, Some(extremum)),
        Some(None) => return Err(TargetlessLoweringError::UnsupportedCollectionPredicate),
    };
    if !domain_filter_is_representable(&predicate) {
        return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
    }
    validate_object_predicate_numeric_expressions(&predicate, declarations)?;
    let count = lower_targetless_numeric_expression(&action.count, declarations)?;
    let mut choice = json!({
        "kind":"chooseCards", "id":decision, "player":player.clone(),
        "from":{
            "kind":"eachPermanent", "player":player,
            "where":lower_permanent_object_predicate(&predicate),
        },
        "count":count,
    });
    if let Some(extremum) = extremum {
        choice["extremum"] = extremum;
    }
    Ok(choice)
}

/// One resolved player chooses, then the chosen permanents are sacrificed as
/// one batch. Choice and performance stay independent primitives; the batch
/// only receives the decision result. `scope` isolates the transient id.
pub(in crate::oracle::canonical) fn lower_player_sacrifice_effects(
    action: &PlayerSacrificeActionNode,
    player: Value,
    scope: &str,
    declarations: TargetlessDeclarationSupport,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let decision = format!("{scope}:decision");
    let choice = lower_player_sacrifice_choice(action, player.clone(), &decision, declarations)?;
    Ok(vec![
        choice,
        json!({
            "kind":"sacrificePermanents", "player":player,
            "objects":{"kind":"decisionResult", "decisionId":decision},
        }),
    ])
}

#[cfg(test)]
#[path = "player_sacrifice_tests.rs"]
mod tests;
