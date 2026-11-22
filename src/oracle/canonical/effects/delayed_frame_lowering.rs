//! Lowering of delayed action ranges (CR 603.7). A delayed range is lowered
//! like any other action, then moved into one delayed step trigger. The
//! trigger performs its actions later, so it may only read results that the
//! program bound before it and must not expose results to later actions.
use std::collections::BTreeSet;

use super::*;

/// Object and player references whose meaning is fixed when the original
/// ability resolves and is not transported into a delayed trigger.
const UNTRANSPORTED_REFERENCES: &[&str] = &[
    "chosenTarget",
    "chosenTargets",
    "chosenObject",
    "triggeringPlayer",
    "triggeringPermanent",
    "triggeringDestination",
    "observedEventObject",
    "eventObject",
    "eventQuantity",
];

#[derive(Default)]
struct BindingUse {
    produced: BTreeSet<String>,
    read: BTreeSet<String>,
    decisions_read: BTreeSet<String>,
    untransported: Option<String>,
    source_object: bool,
}

fn scan(value: &Value, uses: &mut BindingUse) {
    match value {
        Value::Array(values) => values.iter().for_each(|nested| scan(nested, uses)),
        Value::Object(object) => {
            let kind = object.get("kind").and_then(Value::as_str);
            if let Some(bind) = object.get("bind").and_then(Value::as_str) {
                uses.produced.insert(bind.to_string());
            }
            match kind {
                Some("bind" | "chooseNumber" | "chooseOrder" | "chooseCards") => {
                    if let Some(id) = object.get("id").and_then(Value::as_str) {
                        uses.produced.insert(id.to_string());
                    }
                }
                Some("boundObjects" | "singleBoundObject" | "boundObject") => {
                    if let Some(binding) = object.get("binding").and_then(Value::as_str) {
                        uses.read.insert(binding.to_string());
                    }
                }
                Some("boundValue") => {
                    if let Some(id) = object.get("id").and_then(Value::as_str) {
                        uses.read.insert(id.to_string());
                    }
                }
                Some("decisionResult") => {
                    if let Some(id) = object.get("decisionId").and_then(Value::as_str) {
                        uses.decisions_read.insert(id.to_string());
                    }
                }
                Some(kind) if UNTRANSPORTED_REFERENCES.contains(&kind) => {
                    uses.untransported.get_or_insert_with(|| kind.to_string());
                }
                _ => {}
            }
            for (key, nested) in object {
                // The controller of the source is the delayed trigger's own
                // controller (CR 603.7d-e); any other source operand is not.
                if kind == Some("controllerOf") && key == "object" {
                    continue;
                }
                if matches!(nested.get("kind").and_then(Value::as_str), Some("self")) {
                    uses.source_object = true;
                }
                scan(nested, uses);
            }
        }
        _ => {}
    }
}

fn binding_use(values: &[Value]) -> BindingUse {
    let mut uses = BindingUse::default();
    values.iter().for_each(|value| scan(value, &mut uses));
    uses
}

/// Moves each delayed range into an `installDelayedStepTrigger` capturing
/// the earlier bindings it reads. Ranges run last to first so earlier action
/// positions stay valid; later positions are shifted.
pub(super) fn apply_delayed_frames(
    cursor: &mut OracleTokenCursor<'_>,
    frames: &[DelayedFrameNode],
    action_starts: &mut [(usize, usize)],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut frames = frames.to_vec();
    frames.sort_by_key(|frame| std::cmp::Reverse(frame.first_effect));
    for frame in frames {
        let from = action_starts[frame.first_effect].0;
        let to = action_starts[frame.end_effect].0;
        cursor.enter("lower.delayed.actions");
        cursor.require(from < to, "delayed actions that lower to executable steps")?;
        let delayed = effects[from..to].to_vec();
        let inside = binding_use(&delayed);
        cursor.enter("lower.delayed.object-references");
        if inside.untransported.is_some() || inside.source_object {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                "delayed actions reading only results bound earlier, not declared targets, event objects or the source object",
            )));
        }
        let before = binding_use(&effects[..from]);
        let captured = inside
            .read
            .difference(&inside.produced)
            .cloned()
            .collect::<Vec<_>>();
        cursor.require(
            captured
                .iter()
                .all(|binding| before.produced.contains(binding))
                && inside
                    .decisions_read
                    .iter()
                    .all(|decision| inside.produced.contains(decision)),
            "delayed actions whose bound operands are produced earlier in the same program",
        )?;
        cursor.enter("lower.delayed.result-scope");
        let after = binding_use(&effects[to..]);
        cursor.require(
            after
                .read
                .iter()
                .chain(after.decisions_read.iter())
                .all(|binding| !inside.produced.contains(binding)),
            "no later immediate action reading a result of the delayed actions",
        )?;
        let mut install = json!({
            "kind": "installDelayedStepTrigger",
            "step": frame.timing.step.runtime_step(),
            "controller": controller(),
            "effects": delayed,
        });
        if !captured.is_empty() {
            install["captureBindings"] = json!(captured);
        }
        if frame.timing.turn == DelayedStepTurnNode::ControllerTurn {
            install["turnOf"] = controller();
        }
        effects.splice(from..to, [install]);
        for start in action_starts.iter_mut() {
            if start.0 >= to {
                start.0 = start.0 - (to - from) + 1;
            } else if start.0 > from {
                start.0 = from;
            }
        }
    }
    Ok(())
}
