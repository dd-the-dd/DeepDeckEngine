use super::super::*;

/// Player iteration owns the transient player identity. Action children stay
/// ordinary gainLife/loseLife/discardCards/drawCards primitives; explicit
/// controller actions following this node remain outside the iteration.
///
/// Atomic form: exactly one iteration or batch effect. Procedures (sacrifice)
/// and coordinated action lists lower through `lower_quantified_player_actions`.
pub(in crate::oracle::canonical) fn lower_quantified_player_action(
    node: &QuantifiedPlayerActionNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    let mut effects = lower_quantified_player_actions(node, declarations, None)?;
    if effects.len() != 1 {
        return Err(TargetlessLoweringError::UnsupportedEffect);
    }
    Ok(effects.remove(0))
}

/// CR 608.2c: instructions are followed in the order written, and CR 101.4
/// makes the population choose in APNAP order within one instruction. Each
/// action of the shared subject is therefore its own loop or batch that
/// finishes for every player before the next action starts; the engine's
/// iteration is never asked to interleave actions per player. `scope`
/// keeps the transient ids of sibling actions apart.
pub(in crate::oracle::canonical) fn lower_quantified_player_actions(
    node: &QuantifiedPlayerActionNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let mut lowered = Vec::new();
    for (ordinal, action) in node.actions.iter().enumerate() {
        // The first action keeps the caller's scope so single-action IR is
        // unchanged; later actions get a distinct one.
        let action_scope = if ordinal == 0 {
            scope.map(str::to_string)
        } else {
            Some(format!("{}:a{ordinal}", scope.unwrap_or("quantified")))
        };
        lowered.extend(lower_quantified_action(
            node,
            action,
            declarations,
            action_scope.as_deref(),
            ordinal,
        )?);
    }
    Ok(lowered)
}

fn lower_quantified_action(
    node: &QuantifiedPlayerActionNode,
    action: &QuantifiedPlayerAction,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
    ordinal: usize,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let players = lower_quantified_population(node);
    // A sacrifice is a procedure (per-player choice, then one batch).
    if let QuantifiedPlayerAction::Sacrifice(sacrifice) = action {
        return lower_quantified_sacrifice(&players, sacrifice, declarations, scope);
    }
    // A loss action applies to the player set simultaneously; ordinary
    // forEachPlayer remains a sequential program, never an implicit batch.
    if let QuantifiedPlayerAction::Life(life) = action
        && life.action == TargetPlayerLifeAction::Lose
    {
        return Ok(vec![json!({
            "kind":"loseLife", "player":players,
            "amount":lower_targetless_numeric_expression(&life.amount, declarations)?,
        })]);
    }
    let player = json!({"kind":"boundValue", "id":"quantifiedPlayer"});
    let effects = match action {
        QuantifiedPlayerAction::Life(life) => vec![json!({
            "kind":"gainLife", "player":player,
            "amount":lower_targetless_numeric_expression(&life.amount, declarations)?,
        })],
        QuantifiedPlayerAction::Draw(count) => vec![json!({
            "kind":"drawCards", "player":player,
            "count":lower_targetless_numeric_expression(count, declarations)?,
        })],
        QuantifiedPlayerAction::Mill(count) => vec![json!({
            "kind":"mill", "player":player,
            "count":lower_targetless_numeric_expression(count, declarations)?,
        })],
        QuantifiedPlayerAction::Sacrifice(_) => unreachable!("handled by the procedure adapter"),
        QuantifiedPlayerAction::Discard(discard) => {
            // Validate declaration availability/bounds before delegating to
            // the existing discard child and its independent random selector.
            validate_targetless_numeric_expression(&discard.count, declarations)?;
            let random_selection = if ordinal == 0 {
                "quantifiedDiscard".to_string()
            } else {
                format!("quantifiedDiscard{ordinal}")
            };
            lower_player_discard_action_for_player(
                discard,
                PlayerDiscardLoweringIds {
                    target_player: "unusedQuantifiedTarget",
                    random_selection: &random_selection,
                },
                player,
            )
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect)?
        }
    };
    Ok(vec![
        json!({"kind":"forEachPlayer", "players":players, "bind":"quantifiedPlayer", "effects":effects}),
    ])
}

/// `each other player` is every player except the ability controller, which is
/// exactly the controller's opponents in the engine's player model.
fn lower_quantified_population(node: &QuantifiedPlayerActionNode) -> Value {
    if node.excluding_controller {
        lower_player_set_reference(PlayerSetReferenceNode::OpponentsOfAbilityController)
    } else {
        lower_player_set_reference(node.players)
    }
}

/// CR 101.4: every player of the population chooses in APNAP order (the
/// per-player iteration), then all chosen permanents are sacrificed as one
/// simultaneous batch. Choice and performance are independent primitives; the
/// batch only receives the collected selections, never a recomputed candidate
/// set. `scope` keeps the transient ids of sibling actions apart.
fn lower_quantified_sacrifice(
    players: &Value,
    action: &PlayerSacrificeActionNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let id = |name: &str| match scope {
        Some(scope) => format!("{scope}:{name}"),
        None => name.to_string(),
    };
    let player_binding = id("quantifiedPlayer");
    let decision = id("quantifiedSacrifice");
    let collection = id("quantifiedSacrificed");
    let player = json!({"kind":"boundValue", "id":player_binding});
    let choice = lower_player_sacrifice_choice(action, player, &decision, declarations)?;
    Ok(vec![
        json!({
            "kind":"forEachPlayer", "players":players.clone(), "bind":player_binding,
            "effects":[choice],
            "collect":{
                "bind":collection,
                "objects":{"kind":"decisionResult", "decisionId":decision},
            },
        }),
        json!({
            "kind":"sacrificePermanents", "player":players.clone(),
            "objects":{"kind":"boundObjects", "binding":collection},
        }),
    ])
}

#[cfg(test)]
#[path = "quantified_player_action_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "quantified_sacrifice_tests.rs"]
mod sacrifice_tests;
