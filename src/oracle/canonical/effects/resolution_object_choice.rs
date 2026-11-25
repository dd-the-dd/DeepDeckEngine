//! Lowering of the resolution-time permanent and number choices (CR 608.2d)
//! and of the bindings they publish.
//!
//! A permanent choice is one `choosePermanents` decision made by the chooser
//! (the performer of the clause), followed by a `bind` of its result. The
//! chosen objects are published under the action's ordinary symbolic slot
//! key; the shared reference lowering names that key as a target, and
//! [`rebind_chosen_objects`] turns those references into the bound result
//! (`singleBoundObject` for `it`, `boundObjects` for `those creatures`). A
//! player loop (`each player chooses`) republishes the binding of every
//! iteration through its `collect` slot, so a later sentence reads every
//! chosen object. `the rest` is the complement of the choice in each
//! chooser's own domain, bound by the chooser when it chooses and acted on
//! once every chooser has chosen (CR 608.2c).
//!
//! A chosen number is a `chooseNumber` step; `that many` / `that much` binds
//! it through the shared action-result quantity, which installs its `bind`.
//! A choice that no later instruction reads is refused, not a no-op step.
use super::super::*;

const CHOSEN_OBJECTS_SUFFIX: &str = ":chosenObjects";
const UNCHOSEN_OBJECTS_SUFFIX: &str = ":unchosenObjects";
const CHOSEN_NUMBER_SUFFIX: &str = ":chosenNumber";

/// The binding of the permanents chosen by the action at `effect_index`.
fn chosen_objects_binding(effect_index: usize) -> String {
    format!("effect{effect_index}{CHOSEN_OBJECTS_SUFFIX}")
}

/// The binding of the permanents of the choice's domain not chosen.
fn unchosen_objects_binding(effect_index: usize) -> String {
    format!("effect{effect_index}{UNCHOSEN_OBJECTS_SUFFIX}")
}

/// The decision id of the number chosen by the action at `effect_index`.
pub(in crate::oracle::canonical) fn chosen_number_decision(effect_index: usize) -> String {
    format!("effect{effect_index}{CHOSEN_NUMBER_SUFFIX}")
}

/// Whether a runtime action may record a restated quantity: a
/// `chooseNumber` step only when it is a number chosen as its own
/// instruction (an `up to N` count decision belongs to its own action).
pub(in crate::oracle::canonical) fn number_choice_produces(effect: &Value) -> bool {
    effect["kind"] != "chooseNumber"
        || effect["id"]
            .as_str()
            .is_some_and(|id| id.ends_with(CHOSEN_NUMBER_SUFFIX))
}

/// Whether `id` is a binding a permanent choice publishes.
fn is_choice_binding(id: &str) -> bool {
    id.ends_with(CHOSEN_OBJECTS_SUFFIX) || id.ends_with(UNCHOSEN_OBJECTS_SUFFIX)
}

/// The lowered parts of one permanent choice.
pub(in crate::oracle::canonical) struct LoweredObjectChoice {
    /// The choice and its binding, performed by the chooser.
    pub choice: Vec<Value>,
    /// The action on the unchosen permanents, once every chooser chose.
    pub remainder: Vec<Value>,
    /// The chosen-objects binding later references read, when published.
    pub chosen: Option<String>,
}

/// `choose COUNT QUERY [then VERB the rest]` (CR 608.2d). The candidates are
/// the shared permanent query in the chooser's person: `you control` is the
/// chooser's own (a performer runs the step as its controller). A query
/// whose controller is a population (`an opponent controls`) has no
/// candidate writer and is refused.
pub(in crate::oracle::canonical) fn lower_object_choice(
    choice: &ObjectChoiceNode,
    effect_index: usize,
    declarations: TargetlessDeclarationSupport,
) -> Result<LoweredObjectChoice, &'static str> {
    let (predicate, controller_filter) =
        lower_permanent_query_in_context(&choice.query, declarations)
            .map_err(TargetlessLoweringError::expectation)?;
    let query_controller = match controller_filter {
        Some(LoweredQueryController::Player(player)) => Some(player),
        Some(LoweredQueryController::Population(_)) => {
            return Err(
                "a chooser's own or an unrestricted permanent domain; a population of controllers has no candidate writer",
            );
        }
        None => None,
    };
    let minimum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(choice.quantity.minimum),
        declarations,
    )
    .map_err(TargetlessLoweringError::expectation)?;
    let maximum = lower_targetless_numeric_expression(
        &NumericExpression::Atom(choice.quantity.maximum),
        declarations,
    )
    .map_err(TargetlessLoweringError::expectation)?;
    let decision = format!("effect{effect_index}:choosePermanents");
    let mut candidates = json!({"kind": "permanents", "where": predicate.clone()});
    let mut domain = json!({"kind": "eachPermanent", "where": predicate});
    if let Some(player) = query_controller {
        candidates["controller"] = player.clone();
        domain["player"] = player;
    }
    if choice.query.exclude_source {
        candidates["excludeSource"] = json!(true);
        domain["excludeSource"] = json!(true);
    }
    // CR 609.3: with fewer candidates than the count, every candidate is
    // chosen; the decision clamps its bounds to the candidates.
    let mut choose = json!({
        "kind": "choosePermanents",
        "id": decision,
        "player": controller(),
        "candidates": candidates,
        "minimum": minimum,
        "maximum": maximum,
    });
    // `at random` (CR 608.2d): the engine picks the objects itself.
    if choice.random {
        choose["selection"] = json!({"kind": "random"});
    }
    let chosen = decision_result(&decision);
    let Some(action) = &choice.remainder else {
        let binding = chosen_objects_binding(effect_index);
        return Ok(LoweredObjectChoice {
            choice: vec![
                choose,
                json!({"kind": "bind", "id": binding, "value": chosen}),
            ],
            remainder: Vec::new(),
            chosen: Some(binding),
        });
    };
    if let ChoiceRemainderAction::DestroyOthers(others) = action {
        // `destroy all other QUERY`: every permanent of QUERY that no chooser
        // chose, once every chooser has chosen (CR 608.2c).
        let (others, _) = lower_permanent_query_in_context(others, declarations)
            .map_err(TargetlessLoweringError::expectation)?;
        let binding = chosen_objects_binding(effect_index);
        return Ok(LoweredObjectChoice {
            choice: vec![
                choose,
                json!({"kind": "bind", "id": binding, "value": chosen}),
            ],
            remainder: vec![json!({
                "kind": "destroyPermanent",
                "permanent": {
                    "kind": "setDifference",
                    "left": {"kind": "eachPermanent", "where": others},
                    "right": {"kind": "boundObjects", "binding": binding},
                },
            })],
            chosen: None,
        });
    }
    // `the rest`: the chooser's domain without the chosen objects, fixed
    // when the chooser chooses.
    let unchosen = unchosen_objects_binding(effect_index);
    let rest = json!({"kind": "boundObjects", "binding": unchosen});
    let remainder = match action {
        ChoiceRemainderAction::Destroy | ChoiceRemainderAction::DestroyOthers(_) => {
            json!({"kind": "destroyPermanent", "permanent": rest})
        }
        // CR 701.21a: only a permanent's controller sacrifices it. Every
        // chooser sacrifices its own unchosen permanents in one batch.
        ChoiceRemainderAction::Sacrifice => {
            if choice.query.controller != Some(ObjectControllerScope::AbilityController) {
                return Err("the chooser's own permanents as the rest it sacrifices");
            }
            json!({
                "kind": "sacrificePermanents",
                "player": {"kind": "eachPlayer"},
                "objects": rest,
            })
        }
    };
    Ok(LoweredObjectChoice {
        choice: vec![
            choose,
            json!({
                "kind": "bind",
                "id": unchosen,
                "value": {"kind": "setDifference", "left": domain, "right": chosen},
            }),
        ],
        remainder: vec![remainder],
        chosen: None,
    })
}

/// `choose a number between N and M` (CR 608.2d, 107.1): the chooser picks
/// an integer within the bounds. The reader installs `bind`.
pub(in crate::oracle::canonical) fn lower_number_choice(node: NumberChoiceNode, id: &str) -> Value {
    json!({
        "kind": "chooseNumber",
        "id": id,
        "player": controller(),
        "minimum": integer(node.minimum),
        "maximum": integer(node.maximum),
    })
}

/// The `collect` slot of a player loop whose body published a choice
/// binding: the loop republishes the union of every iteration's objects
/// under the same binding (each iteration's bindings are local).
pub(in crate::oracle::canonical) fn player_loop_collection(body: &[Value]) -> Option<Value> {
    let bindings = body
        .iter()
        .filter(|effect| effect["kind"] == "bind")
        .filter_map(|effect| effect["id"].as_str())
        .filter(|id| is_choice_binding(id))
        .collect::<Vec<_>>();
    let [binding] = bindings.as_slice() else {
        return None;
    };
    Some(json!({
        "bind": binding,
        "objects": {"kind": "boundObjects", "binding": binding},
    }))
}

/// The shared reference lowering names the chosen objects by their symbolic
/// slot key as a target; the value is the choice's binding instead.
pub(in crate::oracle::canonical) fn rebind_chosen_objects(value: &mut Value, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let named = value["id"]
        .as_str()
        .filter(|id| ids.iter().any(|candidate| candidate == id))
        .map(str::to_owned);
    if let Some(id) = named {
        match value["kind"].as_str() {
            Some("chosenTarget") => {
                *value = json!({"kind": "singleBoundObject", "binding": id});
                return;
            }
            Some("chosenTargets") => {
                *value = json!({"kind": "boundObjects", "binding": id});
                return;
            }
            _ => {}
        }
    }
    match value {
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| rebind_chosen_objects(value, ids)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| rebind_chosen_objects(value, ids)),
        _ => {}
    }
}

/// Whether `value` reads the objects bound under `id` (a loop's own
/// republication of the binding is no reader).
fn reads_binding(value: &Value, id: &str) -> bool {
    match value {
        Value::Object(fields) => {
            (matches!(
                fields.get("kind").and_then(Value::as_str),
                Some("singleBoundObject" | "boundObjects")
            ) && fields.get("binding").and_then(Value::as_str) == Some(id))
                || fields
                    .iter()
                    .any(|(key, child)| key != "collect" && reads_binding(child, id))
        }
        Value::Array(values) => values.iter().any(|value| reads_binding(value, id)),
        _ => false,
    }
}

/// Whether every published chosen-objects binding is read by the program.
pub(in crate::oracle::canonical) fn every_chosen_object_is_read(
    effects: &[Value],
    ids: &[String],
) -> bool {
    ids.iter()
        .all(|id| effects.iter().any(|effect| reads_binding(effect, id)))
}

/// Whether the `chooseNumber` step with decision `id` received the binding a
/// later `that many` / `that much` installs.
fn number_is_bound(value: &Value, id: &str) -> bool {
    match value {
        Value::Object(fields) => {
            (fields.get("kind").and_then(Value::as_str) == Some("chooseNumber")
                && fields.get("id").and_then(Value::as_str) == Some(id)
                && fields.get("bind").is_some_and(Value::is_string))
                || fields.values().any(|child| number_is_bound(child, id))
        }
        Value::Array(values) => values.iter().any(|value| number_is_bound(value, id)),
        _ => false,
    }
}

/// Whether every number chosen on resolution is read by a later instruction.
pub(in crate::oracle::canonical) fn every_chosen_number_is_read(
    effects: &[Value],
    ids: &[String],
) -> bool {
    ids.iter()
        .all(|id| effects.iter().any(|effect| number_is_bound(effect, id)))
}
