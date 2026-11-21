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
    if !domain_filter_is_representable(&action.object.predicate) {
        return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
    }
    validate_object_predicate_numeric_expressions(&action.object.predicate, declarations)?;
    let count = lower_targetless_numeric_expression(&action.count, declarations)?;
    Ok(json!({
        "kind":"chooseCards", "id":decision, "player":player.clone(),
        "from":{
            "kind":"eachPermanent", "player":player,
            "where":lower_permanent_object_predicate(&action.object.predicate),
        },
        "count":count,
    }))
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
