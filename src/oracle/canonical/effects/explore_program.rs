//! Primitive program for one typed object's exploration procedure.

use super::super::*;
use crate::oracle::ability_catalog::lookup_registry;

fn failure(expected: &str) -> MachineFailure {
    MachineFailure {
        automaton: "effect.explore-program",
        state: "effect.explore-program.registration",
        token_index: 0,
        span_start: 0,
        span_end: 0,
        expected: expected.to_string(),
        actual: None,
        trace: Vec::new(),
    }
}

/// The parent owns the typed reference and supplies a unique local namespace.
/// Object context, library operations, choices, counters and completion remain
/// independent primitives. No runtime branch interprets an explore opcode.
pub(in crate::oracle::canonical) fn lower_explore_program(
    object: Value,
    scope: &str,
) -> MachineResult<Vec<Value>> {
    if scope.is_empty() {
        return Err(failure(
            "a nonempty local scope for the exploration bindings",
        ));
    }
    let registration = lookup_registry("explore")
        .ok_or_else(|| failure("a registered Explore completion backed by CR701.44b"))?;
    if registration.semantics.signal.cardinality != "oncePerAction"
        || registration.semantics.signal.zero_quantity.as_deref() != Some("notApplicable")
    {
        return Err(failure(
            "one completion per exploration, including impossible sub-actions",
        ));
    }
    let subject_id = format!("{scope}:subject");
    let player_id = format!("{scope}:controller");
    let top_id = format!("{scope}:top");
    // The ability's own source keeps its engine-captured last known
    // information; only another object reference is bound.
    let bind_subject = !source_reference(&object);
    let subject = if bind_subject {
        json!({ "kind": "boundValue", "id": subject_id })
    } else {
        object.clone()
    };
    let player = json!({ "kind": "boundValue", "id": player_id });
    let top = json!({ "kind": "boundObjects", "binding": top_id });
    let library = json!({ "kind": "library", "player": player });
    let exists = |objects: Value| {
        json!({
            "kind": "compare",
            "left": { "kind": "countObjects", "objects": objects },
            "operator": ">",
            "right": integer(0),
        })
    };
    let mut signal = lower_registered_ability_signal("explore", player.clone())
        .ok_or_else(|| failure("an afterActionCompletes signal for Explore"))?;
    signal["object"] = subject.clone();
    let mut program = Vec::new();
    if bind_subject {
        program.push(json!({ "kind": "bind", "id": subject_id, "value": object }));
    }
    program.extend([
        json!({
            "kind": "bind", "id": player_id,
            "value": { "kind": "controllerOfObject", "object": subject },
        }),
        json!({
            "kind": "lookAtTopCards", "zone": library,
            "count": integer(1), "bind": top_id,
        }),
        json!({ "kind": "revealCards", "cards": top }),
        json!({
            "kind": "conditional",
            "condition": exists(json!({
                "kind": "filterObjects", "objects": top,
                "where": { "kind": "cardTypeContains", "value": "Land" },
            })),
            "then": [{
                "kind": "moveCards", "cards": top, "from": library,
                "to": { "kind": "hand", "player": player },
            }],
            "else": [{
                "kind": "conditional",
                "condition": { "kind": "objectOnBattlefield", "object": subject },
                "then": [{
                    "kind": "putCounters", "permanent": subject,
                    "counter": "+1/+1", "count": integer(1), "placedBy": player,
                }],
                "else": [],
            }, {
                "kind": "conditional", "condition": exists(top.clone()),
                "then": [{
                    "kind": "optionalEffects", "player": player,
                    "effects": [{
                        "kind": "moveCards", "cards": top, "from": library,
                        "to": { "kind": "graveyard", "player": player },
                    }],
                }],
                "else": [],
            }],
        }),
        signal,
    ]);
    Ok(program)
}

#[cfg(test)]
#[path = "explore_program_tests.rs"]
mod tests;
