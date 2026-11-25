//! Program control of a resolving ability over its own steps (CR 608.2c).
//!
//! ```json
//! {"kind": "repeatableProcess", "process": "repeatedProcess",
//!  "maxRepeats": {"kind": "integer", "value": 1},
//!  "effects": [ ..., {"kind": "repeatProcess", "process": "repeatedProcess"} ]}
//! {"kind": "forEachObject", "objects": ..., "bind": "member",
//!  "effects": [...], "collect": {"bind": "createdTokens", "objects": ...}}
//! ```
//!
//! `repeatProcess` is an ordinary step: reaching it (a conditional or an
//! outcome may govern it) asks the enclosing process to be performed again.
//! Each new performance starts from the bindings and resolution decisions
//! the process started with, so no choice or result of an earlier pass is
//! silently reused. `maxRepeats` is read once, before the first pass. A
//! process that keeps asking to be repeated past the engine's resource limit
//! is refused with an explicit error, never silently stopped.
//!
//! An object iteration binds each member in turn; its body's bindings are
//! the program's, and `collect` republishes the union of a result every pass
//! bound (the tokens each pass created) for the steps after the loop.
use super::*;

/// The most extra performances a repeated process may ask for before the
/// engine refuses it (a resource limit, not a rule of the game).
pub(super) const REPEATED_PROCESS_LIMIT: usize = 512;

fn repetition_signal(process: &str) -> String {
    format!("repeatProcess:{process}")
}

fn contains_repetition_of(value: &Value, process: &str) -> bool {
    match value {
        Value::Object(map) => {
            (value_kind(value) == Some("repeatProcess")
                && value["process"].as_str() == Some(process))
                || map
                    .values()
                    .any(|child| contains_repetition_of(child, process))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| contains_repetition_of(child, process)),
        _ => false,
    }
}

pub(super) fn program_iteration_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("repeatableProcess") => Some(effect["process"].as_str().is_some_and(|process| {
            !process.is_empty()
                && effect
                    .get("maxRepeats")
                    .is_none_or(numeric_expression_supported)
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty()
                        && effects
                            .iter()
                            .any(|child| contains_repetition_of(child, process))
                        && effects
                            .iter()
                            .map(|child| without_repetition_of(child, process))
                            .all(|child| effect_supported(&child))
                })
        })),
        // A repetition is a step of its own process only: validated by the
        // enclosing `repeatableProcess`, never on its own.
        Some("repeatProcess" | "unresolvedRepeatedProcess") => Some(false),
        _ => None,
    }
}

/// A step that does nothing, supported wherever a step is: it stands for a
/// repetition of the process being validated.
fn empty_step() -> Value {
    json!({
        "kind": "conditional",
        "condition": {
            "kind": "compare",
            "operator": "==",
            "left": {"kind": "integer", "value": 0},
            "right": {"kind": "integer", "value": 0},
        },
        "then": [],
        "else": [],
    })
}

/// `value` with every repetition of `process` replaced by an empty step,
/// so the rest of the process is validated as ordinary steps.
fn without_repetition_of(value: &Value, process: &str) -> Value {
    if value_kind(value) == Some("repeatProcess") && value["process"].as_str() == Some(process) {
        return empty_step();
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), without_repetition_of(child, process)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|child| without_repetition_of(child, process))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The `collect` slot of an object iteration: a binding distinct from the
/// member's, republished with the objects every pass bound.
pub(super) fn object_iteration_collect_supported(effect: &Value) -> bool {
    effect.get("collect").is_none_or(|collect| {
        collect["bind"]
            .as_str()
            .is_some_and(|id| !id.is_empty() && Some(id) != effect["bind"].as_str())
            && object_expression_supported(&collect["objects"])
    })
}

impl GameEngine {
    pub(super) fn execute_program_iteration<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        match value_kind(effect) {
            Some("repeatProcess") => {
                let process = effect["process"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("a repetition names its process"))?;
                bindings.insert(repetition_signal(process), RuntimeBinding::Integer(1));
                Ok(true)
            }
            Some("repeatableProcess") => {
                self.execute_repeatable_process(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                    REPEATED_PROCESS_LIMIT,
                )?;
                Ok(true)
            }
            Some("unresolvedRepeatedProcess") => Err(EngineError::new(
                "a repetition whose process is not resolved reached execution",
            )),
            _ => Ok(false),
        }
    }

    /// Performs the process, then performs it again each time a pass
    /// reached its repetition step, up to `maxRepeats` more times when
    /// bounded. More than `limit` extra passes is an explicit refusal.
    pub(super) fn execute_repeatable_process<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
        limit: usize,
    ) -> Result<(), EngineError> {
        let process = effect["process"]
            .as_str()
            .ok_or_else(|| EngineError::new("a repeated process requires an id"))?;
        let signal = repetition_signal(process);
        // `once`, `X more times`: read once, before the first performance.
        let bound = match effect.get("maxRepeats") {
            None => None,
            Some(expression) => Some(
                self.runtime_numeric_expression(expression, stack_object, bindings, decisions)
                    .ok_or_else(|| EngineError::new("a repeated process has an unresolved bound"))?
                    .max(0),
            ),
        };
        let entry_bindings = bindings.clone();
        let entry_decisions = decisions.clone();
        let mut repeated = 0_usize;
        loop {
            bindings.remove(&signal);
            self.execute_effects(
                &effect["effects"],
                stack_object,
                bindings,
                decisions,
                provider,
            )?;
            let requested = bindings.remove(&signal).is_some();
            if !requested || self.state.outcome.is_some() {
                return Ok(());
            }
            if bound.is_some_and(|bound| repeated >= usize::try_from(bound).unwrap_or(0)) {
                return Ok(());
            }
            repeated += 1;
            if repeated > limit {
                return Err(EngineError::new(format!(
                    "a repeated process asked for more than {limit} performances; the engine refuses it"
                )));
            }
            // A new performance: no binding or choice of the last pass.
            *bindings = entry_bindings.clone();
            *decisions = entry_decisions.clone();
        }
    }

    /// One pass per object of the iterated set, the member bound under
    /// `bind` and the enclosing binding restored afterwards; `collect`
    /// publishes the union of a result of every pass.
    pub(super) fn execute_object_iteration<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let objects = self
            .resolve_runtime_object_ids(&effect["objects"], stack_object, bindings, decisions)
            .unwrap_or_default();
        let Some(binding_id) = effect["bind"].as_str() else {
            return Ok(());
        };
        let nested = Value::Array(effect["effects"].as_array().cloned().unwrap_or_default());
        let prior_binding = bindings.get(binding_id).cloned();
        let mut collected = Vec::new();
        let mut seen = BTreeSet::new();
        for instance_id in objects {
            bindings.insert(
                binding_id.to_string(),
                RuntimeBinding::Target(TargetRef::Permanent { instance_id }),
            );
            self.execute_effects(&nested, stack_object, bindings, decisions, provider)?;
            if let Some(collect) = effect.get("collect") {
                let pass = self
                    .resolve_runtime_object_ids(
                        &collect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                for object in pass {
                    if seen.insert(object.clone()) {
                        collected.push(object);
                    }
                }
            }
        }
        if let Some(prior) = prior_binding {
            bindings.insert(binding_id.to_string(), prior);
        } else {
            bindings.remove(binding_id);
        }
        if let Some(id) = effect
            .get("collect")
            .and_then(|collect| collect["bind"].as_str())
        {
            bindings.insert(id.to_string(), RuntimeBinding::Objects(collected));
        }
        Ok(())
    }
}
