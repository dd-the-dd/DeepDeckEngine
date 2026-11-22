use super::super::*;

/// Expand CR701.47a into ordinary actions. The compiler retains the named
/// procedure; the runtime sees only independent primitives and its completion
/// signal. The chosen Army is a resolution decision, never a target.
pub(in crate::oracle::canonical) fn lower_amass_effects(
    effect: &AmassEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let player = match effect.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            return Err(TargetlessLoweringError::RequiresTarget);
        }
    };
    let count = lower_targetless_numeric_expression(&effect.count, declarations)?;
    let subtype = effect.army_subtype.ir_name();
    let choice_id = scope.map_or_else(
        || "amassedArmy".to_string(),
        |scope| format!("{scope}:amassedArmy"),
    );
    let object_id = format!("{choice_id}:object");
    let army_creature = and(vec![
        card_type("Creature"),
        json!({
            "kind": "subtypeContains", "subtypeClass": "creature", "value": "Army",
        }),
    ]);
    let controls_army = json!({
        "kind": "controlsPermanent",
        "where": army_creature,
    });
    let choice = decision_result(&choice_id);
    let signal = lower_registered_ability_signal("amass", player.clone())
        .ok_or(TargetlessLoweringError::UnsupportedEffect)?;
    Ok(vec![
        json!({
            "kind": "conditional", "condition": not(controls_army.clone()),
            "then": [{
                "kind": "createTokens", "controller": player,
                "quantity": integer(1),
                "token": {
                    // The subtype list plus "Token" is its CR111.4 name.
                    "name": format!("{subtype} Army Token"),
                    "colors": ["Black"], "types": ["Creature"],
                    "subtypes": [subtype, "Army"], "power": 0, "toughness": 0,
                },
            }],
        }),
        // Creation may have been replaced or impossible. If there is still no
        // Army creature, skip impossible instructions but complete the action.
        json!({
            "kind": "conditional", "condition": controls_army,
            "then": [
                {
                    "kind": "choosePermanents", "id": choice_id,
                    "player": player,
                    "candidates": { "kind": "permanents", "controller": player, "where": army_creature },
                    "minimum": integer(1), "maximum": integer(1),
                },
                {
                    "kind": "putCounters", "permanent": choice,
                    "counter": "+1/+1", "count": count,
                },
                {
                    "kind": "forEachObject", "objects": choice, "bind": object_id,
                    "effects": [{
                        // Set insertion is idempotent when it is already this subtype.
                        "kind": "addSubtypeToPermanent",
                        "object": { "kind": "boundValue", "id": object_id },
                        "subtype": subtype, "duration": { "kind": "permanent" },
                    }],
                },
            ],
        }),
        // CR701.47b: even N0 or an impossible action still completes amassing.
        signal,
    ])
}

#[cfg(test)]
#[path = "amass_lowering_tests.rs"]
mod tests;
