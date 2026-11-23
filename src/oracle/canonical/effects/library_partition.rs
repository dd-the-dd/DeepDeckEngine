use super::super::*;
use crate::oracle::ability_catalog::lookup_registry;

/// Both procedures inspect a private library prefix, partition it, and restore
/// the kept prefix in a chosen order. The destination is the semantic difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LibraryPartitionAction {
    Scry,
    Surveil,
}

impl LibraryPartitionAction {
    fn ability_id(self) -> &'static str {
        match self {
            Self::Scry => "scry",
            Self::Surveil => "surveil",
        }
    }

    fn local_ids(self) -> [&'static str; 4] {
        match self {
            // Preserve the already-native Scry binding/decision identities.
            Self::Scry => [
                "scryLookedCards",
                "scryBottomCards",
                "scryBottomOrder",
                "scryTopOrder",
            ],
            Self::Surveil => [
                "surveilLookedCards",
                "surveilGraveyardCards",
                "surveilGraveyardOrder",
                "surveilTopOrder",
            ],
        }
    }

    fn selected_destination(self, player: Value) -> Value {
        match self {
            Self::Scry => json!({ "kind": "library", "player": player, "position": "bottom" }),
            Self::Surveil => json!({ "kind": "graveyard", "player": player }),
        }
    }
}

fn partition_failure(expected: &str) -> MachineFailure {
    MachineFailure {
        automaton: "effect.library-partition",
        state: "effect.library-partition.registered-signal",
        token_index: 0,
        span_start: 0,
        span_end: 0,
        expected: expected.to_string(),
        actual: None,
        trace: Vec::new(),
        owner: None,
    }
}

/// Lower a parsed action, never execute catalog text or reconstruct Oracle.
/// `count` is already lowered by the caller's numeric-binding context.
pub(in crate::oracle::canonical) fn lower_library_partition_effects_for_player(
    action: LibraryPartitionAction,
    count_expression: &NumericExpression,
    count: Value,
    player: Value,
    scope: Option<&str>,
) -> MachineResult<Vec<Value>> {
    let ids = action.local_ids().map(|id| {
        scope
            .map(|scope| format!("{scope}:{id}"))
            .unwrap_or_else(|| id.to_string())
    });
    let [looked_id, selected_id, selected_order_id, top_order_id] = ids;
    let looked = json!({ "kind": "boundObjects", "binding": looked_id });
    let selected = decision_result(&selected_id);
    let top = json!({ "kind": "setDifference", "left": looked.clone(), "right": selected.clone() });
    let mut effects = vec![
        json!({
            "kind": "lookAtTopCards",
            "zone": { "kind": "library", "player": player.clone() },
            "count": count.clone(),
            "bind": looked_id,
        }),
        json!({
            "kind": "chooseCards", "id": selected_id, "player": player.clone(),
            "from": looked.clone(), "minimum": integer(0),
            "maximum": { "kind": "countObjects", "objects": looked },
        }),
        json!({
            "kind": "chooseOrder", "id": selected_order_id,
            "player": player.clone(), "objects": selected,
        }),
        json!({
            "kind": "moveCards", "cards": decision_result(&selected_order_id),
            "to": action.selected_destination(player.clone()),
            "order": { "kind": "decisionOrder", "decisionId": selected_order_id },
        }),
        json!({
            "kind": "chooseOrder", "id": top_order_id,
            "player": player.clone(), "objects": top,
        }),
        json!({
            "kind": "moveCards", "cards": decision_result(&top_order_id),
            "to": { "kind": "library", "player": player.clone(), "position": "top" },
            "order": { "kind": "decisionOrder", "decisionId": top_order_id },
        }),
    ];
    let registration = lookup_registry(action.ability_id())
        .ok_or_else(|| partition_failure("a registered library-partition action"))?;
    if registration.semantics.signal.cardinality != "oncePerAction"
        || registration.semantics.signal.zero_quantity.as_deref() != Some("doNotEmit")
    {
        return Err(partition_failure(
            "once-per-action completion with no zero-quantity signal",
        ));
    }
    let signal = lower_registered_ability_signal(action.ability_id(), player)
        .ok_or_else(|| partition_failure("an after-action-completes signal registration"))?;
    match count_expression {
        NumericExpression::Atom(NumericAtom::Integer(value)) if *value <= 0 => {}
        NumericExpression::Atom(NumericAtom::Integer(_)) => effects.push(signal),
        _ => effects.push(json!({
            "kind": "conditional",
            "condition": { "kind": "compare", "operator": ">=", "left": count, "right": integer(1) },
            "then": [signal], "else": [],
        })),
    }
    Ok(effects)
}

#[cfg(test)]
#[path = "library_partition_tests.rs"]
mod tests;
