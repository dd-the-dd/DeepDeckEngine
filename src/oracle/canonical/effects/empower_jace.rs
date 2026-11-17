use super::super::*;

/// Expand CR701.71a into ordinary actions: create the blue Jace token from the
/// shared token catalog when no Jace planeswalker token is controlled, choose
/// one Jace planeswalker token at resolution, then put N loyalty counters on it.
/// The chosen token is a resolution decision, never a target, and no
/// keyword-specific engine operation exists.
pub(in crate::oracle::canonical) fn lower_empower_jace_effects(
    effect: &EmpowerJaceEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let player = match effect.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_) => {
            return Err(TargetlessLoweringError::RequiresTarget);
        }
    };
    let count = lower_targetless_numeric_expression(&effect.count, declarations)?;
    let choice_id = scope.map_or_else(
        || "empoweredJace".to_string(),
        |scope| format!("{scope}:empoweredJace"),
    );
    let jace_token = and(vec![
        json!({ "kind": "isToken" }),
        card_type("Planeswalker"),
        subtype("Jace"),
    ]);
    let controls_jace = json!({
        "kind": "controlsPermanent",
        "where": jace_token,
    });
    Ok(vec![
        json!({
            "kind": "conditional", "condition": not(controls_jace.clone()),
            "then": [{
                "kind": "createTokens", "controller": player,
                "quantity": integer(1),
                // Rules and type line come from the catalog token definition;
                // only its blue color (CR 701.71a) is stated here.
                "token": { "kind": "namedToken", "name": "Jace", "colors": ["Blue"] },
            }],
        }),
        // Creation may have been replaced or impossible: with no Jace token
        // left, skip the impossible instructions.
        json!({
            "kind": "conditional", "condition": controls_jace,
            "then": [
                {
                    "kind": "choosePermanents", "id": choice_id,
                    "player": player,
                    "candidates": { "kind": "permanents", "controller": player, "where": jace_token },
                    "minimum": integer(1), "maximum": integer(1),
                },
                {
                    "kind": "putCounters", "permanent": decision_result(&choice_id),
                    "counter": "loyalty", "count": count,
                },
            ],
        }),
    ])
}

#[cfg(test)]
#[path = "empower_jace_lowering_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "empower_jace_card_tests.rs"]
mod card_tests;
