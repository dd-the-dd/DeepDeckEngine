//! Lowering of effect-created rule ranges (CR 603.7, 614.1a). A governed
//! range is lowered like any other action, then moved into the rule the
//! resolving effect creates: a delayed step trigger, a delayed event trigger
//! or a replacement. A delayed trigger performs its actions later, so it may
//! only read results that the program bound before it, the objects and
//! players it captured when it was created, and its own event; it must not
//! expose results to later actions.
use std::collections::BTreeSet;

use super::*;

/// References whose meaning is fixed when the original ability resolves and
/// that no delayed trigger can capture: a plural declaration, the creating
/// event's destination zone and its quantity.
const UNTRANSPORTED_REFERENCES: &[&str] = &[
    "chosenTargets",
    "triggeringDestination",
    "eventObject",
    "eventQuantity",
];

/// Single objects a delayed trigger captures with their identity when it is
/// created (CR 603.7c): it then acts on them only while each is still the
/// same object (CR 400.7).
const CAPTURED_OBJECT_REFERENCES: &[&str] = &[
    "chosenTarget",
    "chosenObject",
    "triggeringPermanent",
    "observedEventObject",
];

/// Players a delayed trigger captures when it is created.
const CAPTURED_PLAYER_REFERENCES: &[&str] = &["triggeringPlayer"];

/// Which references of a delayed range denote the delayed trigger's own
/// event rather than an event of the ability that created it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EventScope {
    /// A step trigger has no event object (CR 603.7a).
    Step,
    /// The followed object's departure binds the departed object.
    Departure,
    /// A cast record binds its spell and caster.
    SpellCast,
}

impl EventScope {
    fn own_event_reference(self, kind: &str) -> bool {
        match self {
            Self::Step => false,
            Self::Departure => kind == "triggeringPermanent",
            Self::SpellCast => matches!(kind, "triggeringStackObject" | "triggeringPlayer"),
        }
    }
}

/// The references one delayed range captures, in first-read order.
struct Captures {
    prefix: String,
    references: Vec<(Value, String)>,
    untransported: Option<String>,
}

impl Captures {
    fn new(first_effect: usize) -> Self {
        Self {
            prefix: format!("delayedCapture{first_effect}_"),
            references: Vec::new(),
            untransported: None,
        }
    }

    /// One capture per distinct reference.
    fn binding_for(&mut self, reference: Value) -> String {
        if let Some((_, binding)) = self
            .references
            .iter()
            .find(|(captured, _)| *captured == reference)
        {
            return binding.clone();
        }
        let binding = format!("{}{}", self.prefix, self.references.len());
        self.references.push((reference, binding.clone()));
        binding
    }

    fn names(&self) -> BTreeSet<String> {
        self.references
            .iter()
            .map(|(_, binding)| binding.clone())
            .collect()
    }

    fn lowered(&self) -> Value {
        Value::Array(
            self.references
                .iter()
                .map(|(reference, binding)| json!({"binding": binding, "reference": reference}))
                .collect(),
        )
    }
}

/// `self` that names the delayed trigger's source rather than an operand:
/// a damage source, an attachment, or the object of `its controller/owner`
/// (CR 603.7d-e: the delayed trigger has the same source and controller).
fn source_position(key: Option<&str>, parent: Option<&str>) -> bool {
    matches!(key, Some("source" | "attachment"))
        || (key == Some("object") && matches!(parent, Some("controllerOf" | "ownerOf")))
}

/// Replaces every captured reference by the binding the delayed trigger
/// reads at resolution and records the references no trigger can capture.
fn capture_references(
    value: &mut Value,
    key: Option<&str>,
    parent: Option<&str>,
    scope: EventScope,
    captures: &mut Captures,
) {
    let kind = value.get("kind").and_then(Value::as_str).map(str::to_owned);
    if let Some(kind) = kind.as_deref()
        && !scope.own_event_reference(kind)
    {
        let captured = CAPTURED_OBJECT_REFERENCES.contains(&kind)
            || CAPTURED_PLAYER_REFERENCES.contains(&kind)
            || (kind == "self" && !source_position(key, parent));
        if captured {
            let binding = captures.binding_for(value.clone());
            *value = json!({"kind": "boundValue", "id": binding});
            return;
        }
        // The creating event's spell is gone when a step wakes the trigger.
        if UNTRANSPORTED_REFERENCES.contains(&kind)
            || (scope == EventScope::Step && kind == "triggeringStackObject")
        {
            captures
                .untransported
                .get_or_insert_with(|| kind.to_string());
        }
    }
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| capture_references(item, key, parent, scope, captures)),
        Value::Object(object) => {
            for (child_key, child) in object.iter_mut() {
                capture_references(
                    child,
                    Some(child_key.as_str()),
                    kind.as_deref(),
                    scope,
                    captures,
                );
            }
        }
        _ => {}
    }
}

/// True when an action uses the event object other than as the object of
/// `its controller` / `its owner`.
fn operates_on_event_object(value: &Value, parent: Option<&str>) -> bool {
    match value {
        Value::Array(items) => items
            .iter()
            .any(|item| operates_on_event_object(item, parent)),
        Value::Object(object) => {
            let kind = object.get("kind").and_then(Value::as_str);
            if kind == Some("triggeringPermanent") {
                return !matches!(parent, Some("controllerOf" | "ownerOf"));
            }
            object
                .values()
                .any(|child| operates_on_event_object(child, kind))
        }
        _ => false,
    }
}

/// Replaces each occurrence of `from` by `to`.
fn replace_reference(value: &mut Value, from: &Value, to: &Value) {
    if value == from {
        *value = to.clone();
        return;
    }
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| replace_reference(item, from, to)),
        Value::Object(object) => object
            .values_mut()
            .for_each(|child| replace_reference(child, from, to)),
        _ => {}
    }
}

#[derive(Default)]
struct BindingUse {
    produced: BTreeSet<String>,
    read: BTreeSet<String>,
    decisions_read: BTreeSet<String>,
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
                _ => {}
            }
            object.values().for_each(|nested| scan(nested, uses));
        }
        _ => {}
    }
}

fn binding_use(values: &[Value]) -> BindingUse {
    let mut uses = BindingUse::default();
    values.iter().for_each(|value| scan(value, &mut uses));
    uses
}

/// The IR of the object a created rule follows: a declared target the
/// program bound earlier (CR 603.7c, 614.1a).
fn followed_object(
    cursor: &OracleTokenCursor<'_>,
    object: PermanentEffectReference,
    target_bindings: &[(TargetBinding, String)],
) -> Result<Value, EffectSequenceLoweringFailure> {
    match object {
        PermanentEffectReference::Target(_) => {
            lower_bound_permanent_reference(cursor, object, target_bindings)
                .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)
        }
        _ => Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("a declared target as the object the created rule follows"),
        )),
    }
}

/// `If OBJECT would die this turn, exile it instead.` (CR 614.1a): the range
/// is exactly the replacing exile of the followed object.
fn lower_created_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    replacement: &CreatedReplacementNode,
    target_bindings: &[(TargetBinding, String)],
    governed: &[Value],
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.created-replacement.instead");
    cursor.require(
        replacement.instead,
        "a replacement action closed by `instead`",
    )?;
    let object = followed_object(cursor, replacement.object, target_bindings)?;
    cursor.enter("lower.created-replacement.action");
    let exile = json!({"kind": "exilePermanent", "permanent": object.clone()});
    cursor.require(
        governed.len() == 1 && governed[0] == exile,
        "exactly one exile of the object that would die as the replacing action",
    )?;
    Ok(json!({"kind": "installDeathExileReplacement", "object": object}))
}

/// Moves each governed range into the rule its frame creates. Ranges run
/// last to first so earlier action positions stay valid; later positions are
/// shifted.
pub(super) fn apply_delayed_frames(
    cursor: &mut OracleTokenCursor<'_>,
    frames: &[DelayedFrameNode],
    target_bindings: &[(TargetBinding, String)],
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
        let mut delayed = effects[from..to].to_vec();
        let install = if let EffectCreatedRuleNode::Replacement(replacement) = &frame.rule {
            // A replacement is created now and acts on the current object;
            // it captures nothing.
            lower_created_replacement(cursor, replacement, target_bindings, &delayed)?
        } else if let EffectCreatedRuleNode::NextDamage(shield) = &frame.rule {
            lower_next_damage_shield(cursor, shield, &delayed)?
        } else {
            lower_delayed_trigger(
                cursor,
                &frame,
                target_bindings,
                &mut delayed,
                &effects[..from],
                &effects[to..],
            )?
        };
        effects.splice(from..to, [install]);
        for start in action_starts.iter_mut() {
            if start.0 >= to {
                start.0 = start.0 - (to - from) + 1;
            } else if start.0 > from {
                start.0 = from;
            }
        }
    }
    // `prevent that damage` names the damage of a created rule's event; no
    // other sentence supplies one.
    cursor.enter("lower.next-damage.event-damage");
    cursor.require(
        !effects.iter().any(reads_event_damage),
        "`prevent that damage` inside a created next-damage rule",
    )?;
    Ok(())
}

fn reads_event_damage(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(reads_event_damage),
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) == Some("eventDamage")
                || object.values().any(reads_event_damage)
        }
        _ => false,
    }
}

/// `The next time SOURCE of your choice would deal [combat] damage to you
/// this turn, prevent that damage.` (CR 615.7): one prevention shield, used
/// once, whose source the controller chooses on resolution (CR 609.7a).
fn lower_next_damage_shield(
    cursor: &mut OracleTokenCursor<'_>,
    shield: &NextDamageRuleNode,
    governed: &[Value],
) -> Result<Value, EffectSequenceLoweringFailure> {
    let CreatedRuleLifetime::ThisTurn = shield.lifetime;
    let event = &shield.event;
    cursor.enter("lower.next-damage.recipient");
    cursor.require(
        event.recipients == [DamageEventRecipient::Player(DamagePlayerRecipient::You)],
        "you as the protected recipient of the next damage",
    )?;
    cursor.enter("lower.next-damage.amount-guard");
    cursor.require(
        event.minimum.is_none() && event.maximum.is_none(),
        "a next damage event without an amount guard",
    )?;
    cursor.enter("lower.next-damage.kind");
    let combat_only = match event.kind {
        DamageKind::Any => false,
        DamageKind::Combat => true,
        DamageKind::Noncombat => {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("any or combat damage for a next-damage shield"),
            ));
        }
    };
    cursor.enter("lower.next-damage.source");
    let source = event
        .source
        .as_ref()
        .ok_or_else(|| cursor.fail("the damage source the shield names"))?;
    let filter = lower_chosen_damage_source_filter(cursor, source)?;
    cursor.enter("lower.next-damage.action");
    cursor.require(
        governed.len() == 1
            && governed[0]["kind"] == "preventDamage"
            && governed[0]["amount"]["kind"] == "eventDamage"
            && governed[0].get("recipient").is_none()
            && governed[0].get("source").is_none()
            && governed[0]["combatOnly"] == json!(false),
        "exactly `prevent that damage` as the shield's modification",
    )?;
    Ok(json!({
        "kind": "preventDamage",
        "amount": {"kind": "all"},
        "combatOnly": combat_only,
        "recipient": controller(),
        "sourceChoice": {"where": filter},
        "nextTimeOnly": true,
        "duration": {"kind": "untilEndOfCurrentTurn"},
    }))
}

/// One delayed triggered ability over its lowered range (CR 603.7).
fn lower_delayed_trigger(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &DelayedFrameNode,
    target_bindings: &[(TargetBinding, String)],
    delayed: &mut Vec<Value>,
    before: &[Value],
    after: &[Value],
) -> Result<Value, EffectSequenceLoweringFailure> {
    let scope = match &frame.rule {
        EffectCreatedRuleNode::Trigger(trigger) => match &trigger.event {
            DelayedTriggerEventNode::BoundObjectDeparture { .. } => EventScope::Departure,
            DelayedTriggerEventNode::NextSpellCast { .. } => EventScope::SpellCast,
        },
        EffectCreatedRuleNode::Step(_)
        | EffectCreatedRuleNode::Replacement(_)
        | EffectCreatedRuleNode::NextDamage(_) => EventScope::Step,
    };
    // The followed object, read in the trigger's actions, is the object its
    // own departure record binds (CR 603.7c, 603.10a).
    let followed = match &frame.rule {
        EffectCreatedRuleNode::Trigger(EmbeddedDelayedTriggerNode {
            event: DelayedTriggerEventNode::BoundObjectDeparture { object, .. },
            ..
        }) => {
            let followed = followed_object(cursor, *object, target_bindings)?;
            let event_object = json!({"kind": "triggeringPermanent"});
            delayed
                .iter_mut()
                .for_each(|effect| replace_reference(effect, &followed, &event_object));
            // The departed object is a new object in its new zone (CR
            // 400.7): the trigger's actions read only its last known
            // controller or owner, never act on it as a permanent.
            cursor.enter("lower.delayed-trigger.departed-object");
            cursor.require(
                !delayed
                    .iter()
                    .any(|effect| operates_on_event_object(effect, None)),
                "the departed object read only for its last known controller or owner",
            )?;
            Some(followed)
        }
        _ => None,
    };
    cursor.enter("lower.delayed.object-references");
    let mut captures = Captures::new(frame.first_effect);
    delayed
        .iter_mut()
        .for_each(|effect| capture_references(effect, None, None, scope, &mut captures));
    if captures.untransported.is_some() {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
            "delayed actions reading only results bound earlier, single objects and players the trigger captures, or its own event",
        )));
    }
    let inside = binding_use(delayed);
    let captured_names = captures.names();
    let earlier = binding_use(before);
    let captured = inside
        .read
        .difference(&inside.produced)
        .filter(|binding| !captured_names.contains(*binding))
        .cloned()
        .collect::<Vec<_>>();
    cursor.require(
        captured
            .iter()
            .all(|binding| earlier.produced.contains(binding))
            && inside
                .decisions_read
                .iter()
                .all(|decision| inside.produced.contains(decision)),
        "delayed actions whose bound operands are produced earlier in the same program",
    )?;
    cursor.enter("lower.delayed.result-scope");
    let later = binding_use(after);
    cursor.require(
        later
            .read
            .iter()
            .chain(later.decisions_read.iter())
            .all(|binding| !inside.produced.contains(binding)),
        "no later immediate action reading a result of the delayed actions",
    )?;
    let mut install = match &frame.rule {
        EffectCreatedRuleNode::Step(timing) => {
            let mut install = json!({
                "kind": "installDelayedStepTrigger",
                "step": timing.step.runtime_step(),
                "controller": controller(),
                "effects": delayed.clone(),
            });
            if !captured.is_empty() {
                install["captureBindings"] = json!(captured);
            }
            match timing.turn {
                DelayedStepTurnNode::AnyTurn => {}
                DelayedStepTurnNode::ControllerTurn => install["turnOf"] = controller(),
                DelayedStepTurnNode::NextTurn => install["nextTurn"] = json!(true),
            }
            install
        }
        EffectCreatedRuleNode::Trigger(trigger) => {
            cursor.enter("lower.delayed-trigger.bound-results");
            cursor.require(
                captured.is_empty(),
                "delayed trigger actions reading no result an earlier action bound",
            )?;
            let CreatedRuleLifetime::ThisTurn = trigger.lifetime;
            match &trigger.event {
                DelayedTriggerEventNode::BoundObjectDeparture { departure, .. } => {
                    let mut install = json!({
                        "kind": "installDelayedZoneChangeTrigger",
                        "controller": controller(),
                        "trackedObject": followed
                            .clone()
                            .expect("a departure trigger follows its object"),
                        "duration": {"kind": "untilEndOfCurrentTurn"},
                        "effects": delayed.clone(),
                    });
                    match departure {
                        ObjectDeparture::Dies => install["destinations"] = json!(["graveyard"]),
                        ObjectDeparture::LeavesBattlefield => {
                            install["anyDestination"] = json!(true)
                        }
                    }
                    install
                }
                DelayedTriggerEventNode::NextSpellCast { predicate } => {
                    cursor.enter("lower.delayed-trigger.spell-cast.captures");
                    cursor.require(
                        captures.references.is_empty(),
                        "a next-spell trigger reading only its own cast record",
                    )?;
                    json!({
                        "kind": "installDelayedSpellCastTrigger",
                        "player": controller(),
                        "where": lower_spell_object_predicate(predicate).unwrap_or(Value::Null),
                        "duration": {"kind": "untilEndOfCurrentTurn"},
                        "effects": delayed.clone(),
                    })
                }
            }
        }
        EffectCreatedRuleNode::Replacement(_) | EffectCreatedRuleNode::NextDamage(_) => {
            unreachable!("a created replacement is lowered without a delayed trigger")
        }
    };
    if !captures.references.is_empty() {
        install["captureReferences"] = captures.lowered();
    }
    Ok(install)
}
