//! `objectMatchesFilter`: one referenced object tested against the shared
//! object filter when the condition is evaluated during resolution.
//!
//! CR 608.2h: an object still in the zone it was expected to be in is read
//! with its current (effective) characteristics. A designated permanent that
//! has left the battlefield would need last-known information, which this
//! condition does not capture: the checked path reports it instead of
//! reading the new object in its new zone.
use super::object_predicate::{DefinitionView, compare_integers, try_evaluate_predicate};
use super::runtime_object_filter::ObjectFilterView;
use super::*;

/// The object references an object condition accepts: the ability source,
/// a target or bound singular reference, the object observed by an entry
/// event, or a bound/decided collection that must hold at most one object.
pub(super) fn object_condition_reference_supported(reference: &Value) -> bool {
    matches!(value_kind(reference), Some("self" | "abilitySource"))
        || target_reference_supported(reference)
        || entry_observation::observed_object_reference_supported(reference)
        || (matches!(
            value_kind(reference),
            Some("boundObjects" | "decisionResult")
        ) && object_expression_supported(reference))
}

enum ConditionSubject<'a> {
    View(ObjectFilterView<'a>),
    /// The event's permanent after it left the battlefield, as the event
    /// record last saw it (CR 603.10a, 608.2h): its counters, combat state
    /// and power/toughness captured when it died.
    LastKnown(LastKnownEventObject),
    /// No object (no card looked at, an unchosen optional target): the
    /// object has no characteristic, so the condition is false.
    Absent,
}

struct LastKnownEventObject {
    card: CardInstance,
    was_creature: bool,
    power: i32,
    toughness: i32,
}

/// The death record's last-known event object, rebuilt from the record's
/// own captures over the card it became. `None` without a death record.
fn last_known_event_object(
    card: &CardInstance,
    decisions: &BTreeMap<String, Value>,
) -> Option<LastKnownEventObject> {
    let counters = decisions.get("triggeringPermanentCounters")?;
    let integer = |key: &str| {
        decisions
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
    };
    let flag = |key: &str| decisions.get(key).and_then(Value::as_bool);
    let mut card = card.clone();
    card.counters = serde_json::from_value(counters.clone()).ok()?;
    if let Some(controller) = decisions
        .get("triggeringPermanentController")
        .and_then(Value::as_str)
    {
        card.controller = controller.to_string();
    }
    // Combat status and designations (`renowned`, `monstrous`) as it last
    // existed on the battlefield.
    card.flags = serde_json::from_value(decisions.get("triggeringPermanentFlags")?.clone()).ok()?;
    Some(LastKnownEventObject {
        card,
        was_creature: flag("triggeringPermanentWasCreature")?,
        power: integer("triggeringPermanentPower")?,
        toughness: integer("triggeringPermanentToughness")?,
    })
}

impl GameEngine {
    fn permanent_or_card_view<'a>(&'a self, instance_id: &str) -> ConditionSubject<'a> {
        if let Some((player, index)) = self.permanent_position(instance_id) {
            return ConditionSubject::View(ObjectFilterView::Permanent(
                &self.state.players[player].battlefield[index],
            ));
        }
        match self.card_outside_battlefield(instance_id) {
            Some(card) => ConditionSubject::View(ObjectFilterView::Definition(
                DefinitionView::new(&card.definition),
            )),
            None => ConditionSubject::Absent,
        }
    }

    fn object_condition_subject<'a>(
        &'a self,
        reference: &Value,
        stack_object: &'a StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<ConditionSubject<'a>, EngineError> {
        match value_kind(reference) {
            Some("self" | "abilitySource") => {
                if !source_lki::source_has_departed(stack_object)
                    && let Some((player, index)) =
                        self.permanent_position(&stack_object.card.instance_id)
                {
                    return Ok(ConditionSubject::View(ObjectFilterView::Permanent(
                        &self.state.players[player].battlefield[index],
                    )));
                }
                // CR 603.10a: a source that left the battlefield is read as it
                // last existed there (the authenticated departure capture,
                // with its counters), not as its printed card.
                if source_lki::source_has_departed(stack_object) {
                    return Ok(ConditionSubject::View(ObjectFilterView::Permanent(
                        &stack_object.card,
                    )));
                }
                Ok(ConditionSubject::View(ObjectFilterView::Definition(
                    DefinitionView::new(&stack_object.card.definition),
                )))
            }
            Some("observedEventObject") => Ok(
                match self.resolve_observed_event_permanent(reference, stack_object)? {
                    Some(instance_id) => self.permanent_or_card_view(&instance_id),
                    None => ConditionSubject::Absent,
                },
            ),
            Some("triggeringPermanent")
                if self
                    .resolve_target(reference, stack_object, bindings)
                    .is_none()
                    && stack_object
                        .decisions
                        .contains_key("triggeringPermanentCounters") =>
            {
                // The event's permanent died: read the death record's
                // last-known object, never the card in its new zone.
                let instance_id = stack_object.decisions["triggeringPermanentId"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("a death record without its permanent"))?;
                let card = self.card_outside_battlefield(instance_id).ok_or_else(|| {
                    EngineError::new("a dead event object whose card cannot be found")
                })?;
                last_known_event_object(card, &stack_object.decisions)
                    .map(ConditionSubject::LastKnown)
                    .ok_or_else(|| EngineError::new("an incomplete death record"))
            }
            Some("boundObjects" | "decisionResult") => {
                let objects = self.resolve_runtime_object_ids_checked(
                    reference,
                    stack_object,
                    bindings,
                    decisions,
                )?;
                match objects.as_slice() {
                    [] => Ok(ConditionSubject::Absent),
                    [instance_id] => Ok(self.permanent_or_card_view(instance_id)),
                    _ => Err(EngineError::new(
                        "an object condition requires at most one referenced object",
                    )),
                }
            }
            _ => Ok(
                match self.resolve_target(reference, stack_object, bindings) {
                    None | Some(TargetRef::Player { .. }) => ConditionSubject::Absent,
                    Some(TargetRef::Permanent { instance_id }) => {
                        let Some((player, index)) = self.permanent_position(&instance_id) else {
                            return Err(EngineError::new(
                                "an object condition over a permanent that left the battlefield needs its last-known information",
                            ));
                        };
                        ConditionSubject::View(ObjectFilterView::Permanent(
                            &self.state.players[player].battlefield[index],
                        ))
                    }
                    Some(TargetRef::Card { instance_id }) => {
                        self.permanent_or_card_view(&instance_id)
                    }
                    Some(TargetRef::StackObject { stack_id }) => self
                        .state
                        .stack
                        .iter()
                        .find(|object| object.id == stack_id)
                        .map_or(ConditionSubject::Absent, |object| {
                            ConditionSubject::View(ObjectFilterView::Definition(
                                DefinitionView::new(&object.card.definition),
                            ))
                        }),
                },
            ),
        }
    }

    /// Checked evaluation: a missing binding, an ambiguous collection, a
    /// departed permanent or an invalid numeric operand is an error.
    pub(super) fn checked_object_condition_matches(
        &self,
        condition: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        let subject =
            self.object_condition_subject(&condition["object"], stack_object, bindings, decisions)?;
        let view = match &subject {
            ConditionSubject::View(view) => *view,
            ConditionSubject::LastKnown(object) => {
                return try_evaluate_predicate(&condition["where"], &mut |leaf| {
                    self.last_known_condition_leaf(object, leaf, stack_object, bindings, decisions)
                });
            }
            ConditionSubject::Absent => return Ok(false),
        };
        try_evaluate_predicate(&condition["where"], &mut |leaf| {
            self.object_condition_leaf(view, leaf, stack_object, bindings, decisions)
        })
    }

    /// A leaf over the last-known event object: its captured power and
    /// toughness, and its other atoms on the rebuilt battlefield view.
    fn last_known_condition_leaf(
        &self,
        object: &LastKnownEventObject,
        leaf: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(leaf) == Some("compare")
            && value_kind(&leaf["left"]["object"]) == Some("candidate")
            && let Some(left) = match value_kind(&leaf["left"]) {
                Some("powerOf") => Some(object.power),
                Some("toughnessOf") => Some(object.toughness),
                _ => None,
            }
        {
            // CR 208.3: an object without power/toughness never matches.
            if !object.was_creature {
                return Ok(false);
            }
            let right = self.checked_runtime_numeric_expression(
                &leaf["right"],
                stack_object,
                bindings,
                decisions,
                "objectMatchesFilter.compare.right",
            )?;
            return Ok(compare_integers(left, leaf["operator"].as_str(), right));
        }
        self.object_condition_leaf(
            ObjectFilterView::Permanent(&object.card),
            leaf,
            stack_object,
            bindings,
            decisions,
        )
    }

    fn object_condition_leaf(
        &self,
        view: ObjectFilterView<'_>,
        leaf: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(leaf) == Some("compare") {
            if value_kind(&leaf["left"]["object"]) != Some("candidate") {
                return Err(EngineError::new(
                    "an object condition compares a property of its referenced object",
                ));
            }
            let right = self.checked_runtime_numeric_expression(
                &leaf["right"],
                stack_object,
                bindings,
                decisions,
                "objectMatchesFilter.compare.right",
            )?;
            let stat = match value_kind(&leaf["left"]) {
                Some("powerOf") => Some(true),
                Some("toughnessOf") => Some(false),
                _ => None,
            };
            if let Some(power) = stat {
                // CR 208.3: an object without power/toughness never matches.
                let left = match view {
                    ObjectFilterView::Permanent(card) => {
                        if !self.permanent_is_creature(card) {
                            return Ok(false);
                        }
                        if power {
                            self.current_power(card)
                        } else {
                            self.current_toughness(card)
                        }
                    }
                    ObjectFilterView::Definition(_) => {
                        let definition = view.definition();
                        let printed = if power {
                            &definition.power
                        } else {
                            &definition.toughness
                        };
                        let Some(printed) = printed.as_deref() else {
                            return Ok(false);
                        };
                        // A characteristic-defining P/T outside the battlefield
                        // has no evaluator here; never read it as zero.
                        printed.parse::<i32>().map_err(|_| {
                            EngineError::new(
                                "an object condition over a non-numeric printed power or toughness",
                            )
                        })?
                    }
                };
                return Ok(compare_integers(left, leaf["operator"].as_str(), right));
            }
        }
        Ok(self.runtime_object_filter_atom_matches(view, leaf, stack_object, bindings, decisions))
    }

    /// Infallible consumers keep the historical policy: an unevaluable
    /// condition is false. The checked conditional path reports the error.
    pub(super) fn object_condition_matches(
        &self,
        condition: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        self.checked_object_condition_matches(condition, stack_object, bindings, decisions)
            .unwrap_or(false)
    }
}
