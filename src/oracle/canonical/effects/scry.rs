use super::super::*;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

pub(in crate::oracle::canonical) fn claims_promoted_scry_family(input: &AbilityInput<'_>) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("scry"))
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

fn scoped_scry_id(scope: Option<&str>, default: &str) -> String {
    scope
        .map(|scope| format!("{scope}:{default}"))
        .unwrap_or_else(|| default.to_string())
}

/// Lowers the rule definition of scry for an already-resolved player. Modal
/// parents provide a chosen target and a scope so all transient bindings and
/// choices remain independent from other selected modes.
pub(in crate::oracle::canonical) fn lower_scry_effects_for_player(
    count_expression: &NumericExpression,
    player: Value,
    scope: Option<&str>,
) -> MachineResult<Vec<Value>> {
    let count = lower_numeric_expression(count_expression).ok_or_else(|| MachineFailure {
        automaton: "effect.scry",
        state: "effect.scry.lower-count",
        token_index: 0,
        span_start: 0,
        span_end: 0,
        expected: "a target-independent numeric expression".to_string(),
        actual: None,
        trace: Vec::new(),
    })?;
    let looked_id = scoped_scry_id(scope, "scryLookedCards");
    let bottom_id = scoped_scry_id(scope, "scryBottomCards");
    let bottom_order_id = scoped_scry_id(scope, "scryBottomOrder");
    let top_order_id = scoped_scry_id(scope, "scryTopOrder");
    let looked = json!({ "kind": "boundObjects", "binding": looked_id });
    let bottom = decision_result(&bottom_id);
    let bottom_order = decision_result(&bottom_order_id);
    let top = json!({
        "kind": "setDifference",
        "left": looked.clone(),
        "right": bottom.clone(),
    });
    let top_order = decision_result(&top_order_id);
    let mut effects = vec![
        json!({
            "kind": "lookAtTopCards",
            "zone": { "kind": "library", "player": player.clone() },
            "count": count.clone(),
            "bind": looked_id,
        }),
        json!({
            "kind": "chooseCards",
            "id": bottom_id,
            "player": player.clone(),
            "from": looked,
            "minimum": integer(0),
            "maximum": {
                "kind": "countObjects",
                "objects": { "kind": "boundObjects", "binding": looked_id },
            },
        }),
        json!({
            "kind": "chooseOrder",
            "id": bottom_order_id,
            "player": player.clone(),
            "objects": bottom,
        }),
        json!({
            "kind": "moveCards",
            "cards": bottom_order,
            "to": { "kind": "library", "player": player.clone(), "position": "bottom" },
            "order": { "kind": "decisionOrder", "decisionId": bottom_order_id },
        }),
        json!({
            "kind": "chooseOrder",
            "id": top_order_id,
            "player": player.clone(),
            "objects": top,
        }),
        json!({
            "kind": "moveCards",
            "cards": top_order,
            "to": { "kind": "library", "player": player.clone(), "position": "top" },
            "order": { "kind": "decisionOrder", "decisionId": top_order_id },
        }),
    ];
    if let Some(signal) = lower_registered_ability_signal("scry", player) {
        match count_expression {
            NumericExpression::Atom(NumericAtom::Integer(0)) => {}
            NumericExpression::Atom(NumericAtom::Integer(_)) => effects.push(signal),
            _ => effects.push(json!({
                "kind": "conditional",
                "condition": {
                    "kind": "compare",
                    "operator": ">=",
                    "left": count,
                    "right": integer(1),
                },
                "then": [signal],
                "else": [],
            })),
        }
    }
    Ok(effects)
}

fn lower_scry_effects(effect: &ScryEffectNode) -> MachineResult<Vec<Value>> {
    let player = match &effect.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) => {
            unreachable!("standalone scry recognizer binds controller")
        }
    };
    lower_scry_effects_for_player(&effect.count, player, None)
}

/// Promotes a complete, context-free scry instruction for legacy ability
/// envelopes that already delegate their effect body to the generic
/// instruction parser. Recognition still goes through the token automaton;
/// this adapter exists only while those parent envelopes are migrated.
pub(in crate::oracle::canonical) fn parse_scry_effect_instruction(
    instruction: &str,
) -> Option<Vec<Value>> {
    let tokens = recognize_entities(instruction);
    let mut cursor = OracleTokenCursor::new("scry-instruction", &tokens);
    let effect = recognize_scry_effect(&mut cursor).ok()?;
    cursor.enter("effect.scry.period");
    cursor.consume(".").ok()?;
    if cursor.peek() == Some("(") {
        recognize_scry_reminder(&mut cursor, &effect.count).ok()?;
    }
    cursor.enter("effect.scry.validation");
    cursor.finish().ok()?;
    lower_scry_effects(&effect).ok()
}

pub(in crate::oracle::canonical) fn parse_promoted_scry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("scry-action", input.tokens);
    let effect = recognize_scry_effect(&mut cursor)?;
    cursor.enter("effect.scry.period");
    cursor.consume(".")?;
    if cursor.peek() == Some("(") {
        recognize_scry_reminder(&mut cursor, &effect.count)?;
    }
    cursor.enter("effect.scry.validation");
    cursor.finish()?;
    let effects = lower_scry_effects(&effect)?;
    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": effects,
    });
    if matches!(
        &effect.count,
        NumericExpression::Atom(NumericAtom::VariableX)
    ) {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [x_value()],
        });
    }
    Ok(draft(
        rule,
        &[
            "Recognize scry and its numeric expression from the shared token stream",
            "Partition the looked-at cards into ordered bottom and top collections",
            "Emit the catalog-registered scry signal only for a positive quantity",
        ],
    ))
}
