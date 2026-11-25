//! `objectMatchesFilter`: one referenced object tested against the shared
//! object filter when the condition is evaluated during resolution.
//!
//! CR 608.2h: an object still in the zone it was expected to be in is read
//! with its current (effective) characteristics. A designated permanent that
//! has left the battlefield would need last-known information, which this
//! condition does not capture: the checked path reports it instead of
//! reading the new object in its new zone.
//!
//! An object a cost paid with (`the sacrificed creature`, `the discarded
//! card`) is recorded by its paying cost when it is paid (`bindObjectAs`)
//! and read from that record, as last known information (CR 608.2h). A
//! relation leaf of the filter (`shares a creature type with this
//! creature`) reads its anchor from the resolving ability first.
use super::object_predicate::{DefinitionView, compare_integers, try_evaluate_predicate};
use super::runtime_object_filter::ObjectFilterView;
use super::*;

/// The stack values a paying cost records for an object condition
/// (`bindObjectAs`, CR 601.2h, 602.2b, 608.2h): the sacrificed permanent as
/// it last existed on the battlefield, and the one discarded card.
const COST_PAID_OBJECT_DECISIONS: [&str; 2] = ["sacrificedPermanentObject", "discardedCardObject"];

/// `{ "kind": "decisionResult", "decisionId": <cost-paid object> }`.
fn cost_paid_object_reference(reference: &Value) -> Option<&str> {
    (value_kind(reference) == Some("decisionResult"))
        .then(|| reference["decisionId"].as_str())
        .flatten()
        .filter(|id| COST_PAID_OBJECT_DECISIONS.contains(id))
}

/// The filter of an object condition: the shared permanent filter, whose
/// relation leaves (`shares a creature type with this creature`) are
/// prepared against the resolving ability before the object is tested.
pub(super) fn object_condition_filter_supported(filter: &Value) -> bool {
    super::object_relation_eval::filter_supported_with_relations(
        filter,
        object_condition_leaf_supported,
    )
}

/// One leaf of an object condition. A comparison of the referenced object's
/// property reads its other operand from the resolving ability, the
/// source's own power or toughness included (`if it has greater power than
/// this creature`, CR 608.2h), unlike a collection filter, whose candidates
/// have no source binder. Every other leaf is the shared permanent filter's.
fn object_condition_leaf_supported(leaf: &Value) -> bool {
    if value_kind(leaf) == Some("compare")
        && matches!(
            value_kind(&leaf["left"]),
            Some("powerOf" | "toughnessOf" | "manaValueOf")
        )
        && value_kind(&leaf["left"]["object"]) == Some("candidate")
    {
        return numeric_expression_supported(&leaf["right"])
            && matches!(
                leaf["operator"].as_str(),
                Some(">=" | ">" | "<=" | "<" | "==")
            );
    }
    permanent_filter_supported(leaf)
}

/// The object references an object condition accepts: the ability source,
/// a target or bound singular reference, the object observed by an entry
/// event, an object a cost of the ability paid with, or a bound/decided
/// collection that must hold at most one object.
pub(super) fn object_condition_reference_supported(reference: &Value) -> bool {
    cost_paid_object_reference(reference).is_some()
        || matches!(value_kind(reference), Some("self" | "abilitySource"))
        || target_reference_supported(reference)
        || entry_observation::observed_object_reference_supported(reference)
        || (matches!(
            value_kind(reference),
            Some("boundObjects" | "decisionResult")
        ) && object_expression_supported(reference))
}

enum ConditionSubject<'a> {
    View(ObjectFilterView<'a>),
    /// A card outside the battlefield: its printed definition, and the card
    /// itself as the source of its characteristic-defining abilities.
    Card(&'a CardInstance),
    /// The event's permanent after it left the battlefield, as the event
    /// record last saw it (CR 603.10a, 608.2h): its counters, combat state
    /// and power/toughness captured when it died. A permanent a cost
    /// sacrificed is read the same way, from its cost record.
    LastKnown(LastKnownEventObject),
    /// The card a cost discarded, as its cost record captured it.
    RecordedCard(CardInstance),
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

/// The record a paying cost writes for its object: the object as it was
/// when the cost was paid, and for a permanent its creature status and
/// power/toughness then (CR 608.2h).
fn cost_paid_object_record(
    card: &CardInstance,
    permanent: Option<(bool, i32, i32)>,
) -> Option<Value> {
    let mut record = json!({
        "kind": "costPaidObject",
        "card": serde_json::to_value(card).ok()?,
        "permanent": permanent.is_some(),
    });
    if let Some((was_creature, power, toughness)) = permanent {
        record["wasCreature"] = json!(was_creature);
        record["power"] = json!(power);
        record["toughness"] = json!(toughness);
    }
    Some(record)
}

/// The subject a cost record names: the sacrificed permanent's last known
/// object, or the discarded card.
fn recorded_cost_object<'a>(record: &Value) -> Result<ConditionSubject<'a>, EngineError> {
    let malformed = || EngineError::new("a malformed cost-paid object record");
    if value_kind(record) != Some("costPaidObject") {
        return Err(malformed());
    }
    let card: CardInstance =
        serde_json::from_value(record["card"].clone()).map_err(|_| malformed())?;
    if record["permanent"].as_bool() != Some(true) {
        return Ok(ConditionSubject::RecordedCard(card));
    }
    let integer = |key: &str| {
        record[key]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(malformed)
    };
    Ok(ConditionSubject::LastKnown(LastKnownEventObject {
        was_creature: record["wasCreature"].as_bool().ok_or_else(malformed)?,
        power: integer("power")?,
        toughness: integer("toughness")?,
        card,
    }))
}

impl GameEngine {
    /// Records the objects the costs of an activation or the additional
    /// costs of a spell pay with, for the costs whose object a condition of
    /// the same ability reads (`bindObjectAs`). Called before the costs are
    /// paid: a sacrificed permanent is captured as it exists on the
    /// battlefield, a chosen discarded card as the card it is. A random
    /// discard is recorded once its payment selected the card
    /// (`record_random_discard_cost_object`).
    pub(super) fn record_cost_paid_objects(
        &self,
        costs: &Value,
        targets: &BTreeMap<String, TargetRef>,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        self.record_cost_paid_card_properties(costs, targets, decisions);
        for cost in costs.as_array().into_iter().flatten() {
            let Some(binding) = cost["bindObjectAs"].as_str() else {
                continue;
            };
            let record = match value_kind(cost) {
                Some("sacrificePermanent") => cost["permanent"]["id"]
                    .as_str()
                    .and_then(|id| targets.get(id))
                    .and_then(|target| match target {
                        TargetRef::Permanent { instance_id } => {
                            self.permanent_position(instance_id)
                        }
                        _ => None,
                    })
                    .and_then(|(player, index)| {
                        let permanent = &self.state.players[player].battlefield[index];
                        cost_paid_object_record(
                            permanent,
                            Some((
                                self.permanent_is_creature(permanent),
                                self.current_power(permanent),
                                self.current_toughness(permanent),
                            )),
                        )
                    }),
                Some("discardCard") => cost["card"]["id"]
                    .as_str()
                    .and_then(|id| targets.get(id))
                    .and_then(|target| match target {
                        TargetRef::Card { instance_id } => {
                            self.card_outside_battlefield(instance_id)
                        }
                        _ => None,
                    })
                    .and_then(|card| cost_paid_object_record(card, None)),
                _ => None,
            };
            if let Some(record) = record {
                decisions.insert(binding.to_string(), record);
            }
        }
    }

    /// The spell form of [`Self::record_cost_paid_objects`]: the additional
    /// costs of the spell's own rules (CR 601.2b, 601.2h).
    pub(super) fn record_casting_cost_paid_objects(
        &self,
        card: &CardDefinition,
        targets: &BTreeMap<String, TargetRef>,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        for rule in card
            .printed_rules()
            .iter()
            .filter(|rule| value_kind(rule) == Some("spellAbility"))
        {
            self.record_cost_paid_objects(
                &rule["declaration"]["additionalCosts"],
                targets,
                decisions,
            );
        }
    }

    /// Records the one card a random discard cost of the activation that
    /// was just put on the stack discarded (CR 701.9b), from the selection
    /// its payment published, on that stack object.
    pub(super) fn record_random_discard_cost_object(&mut self, rule: &Value) {
        let Some(binding) = rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| value_kind(cost) == Some("discardCards"))
            .find_map(|cost| cost["bindObjectAs"].as_str())
            .map(ToOwned::to_owned)
        else {
            return;
        };
        let Some(source_id) = self
            .state
            .stack
            .last()
            .map(|object| object.card.instance_id.clone())
        else {
            return;
        };
        // The payment of this activation published its selection after the
        // previous activation was put on the stack.
        let selected = self
            .state
            .events
            .iter()
            .rev()
            .take_while(|event| event.kind != "activatedAbilityPutOnStack")
            .find(|event| {
                event.kind == "cardsRandomlySelected"
                    && event.detail["reason"].as_str() == Some("cost")
                    && event.card_instance_id.as_deref() == Some(source_id.as_str())
            })
            .and_then(|event| event.detail["cardIds"].as_array())
            .and_then(|ids| match ids.as_slice() {
                [id] => id.as_str().map(ToOwned::to_owned),
                _ => None,
            });
        let Some(record) = selected
            .as_deref()
            .and_then(|id| self.card_outside_battlefield(id))
            .and_then(|card| cost_paid_object_record(card, None))
        else {
            return;
        };
        if let Some(object) = self.state.stack.last_mut() {
            object.decisions.insert(binding, record);
        }
    }

    /// Prepares the relation leaves of an object condition's filter against
    /// the resolving ability (CR 608.2b): the anchor's characteristic values
    /// are read once, then the referenced object is tested.
    fn prepare_object_condition_filter(
        &self,
        filter: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Value, EngineError> {
        if !super::object_relation_eval::filter_has_relation(filter) {
            return Ok(filter.clone());
        }
        super::object_predicate::try_map_predicate_leaves(filter, 0, &mut |leaf| match value_kind(
            leaf,
        ) {
            Some(kind) if super::object_relation_eval::is_relation_kind(kind) => {
                self.prepare_object_relation(leaf, stack_object, bindings, decisions)
            }
            _ => Ok(leaf.clone()),
        })
    }

    fn permanent_or_card_view<'a>(&'a self, instance_id: &str) -> ConditionSubject<'a> {
        if let Some((player, index)) = self.permanent_position(instance_id) {
            return ConditionSubject::View(ObjectFilterView::Permanent(
                &self.state.players[player].battlefield[index],
            ));
        }
        match self.card_outside_battlefield(instance_id) {
            Some(card) => ConditionSubject::Card(card),
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
        if let Some(decision) = cost_paid_object_reference(reference) {
            // CR 608.2h: the object the cost paid with, as its paying cost
            // recorded it. A paid cost always records it; its absence is an
            // error, never an object without characteristics.
            let record = stack_object.decisions.get(decision).ok_or_else(|| {
                EngineError::new("the cost-paid object its paying cost did not record")
            })?;
            return recorded_cost_object(record);
        }
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
                Ok(ConditionSubject::Card(&stack_object.card))
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
        let filter = self.prepare_object_condition_filter(
            &condition["where"],
            stack_object,
            bindings,
            decisions,
        )?;
        let (view, card) = match &subject {
            ConditionSubject::View(view) => (*view, None),
            ConditionSubject::Card(card) => (
                ObjectFilterView::Definition(DefinitionView::new(&card.definition)),
                Some(*card),
            ),
            ConditionSubject::RecordedCard(card) => (
                ObjectFilterView::Definition(DefinitionView::new(&card.definition)),
                Some(card),
            ),
            ConditionSubject::LastKnown(object) => {
                return try_evaluate_predicate(&filter, &mut |leaf| {
                    self.last_known_condition_leaf(object, leaf, stack_object, bindings, decisions)
                });
            }
            ConditionSubject::Absent => return Ok(false),
        };
        try_evaluate_predicate(&filter, &mut |leaf| {
            self.object_condition_leaf(view, card, leaf, stack_object, bindings, decisions)
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
            None,
            leaf,
            stack_object,
            bindings,
            decisions,
        )
    }

    fn object_condition_leaf(
        &self,
        view: ObjectFilterView<'_>,
        card: Option<&CardInstance>,
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
                        // CR 604.3: a characteristic-defining P/T functions in
                        // every zone (layer 7a); a non-numeric printed value
                        // without one is never read as zero.
                        match card.and_then(|card| self.characteristic_defining_power_toughness(card)) {
                            Some((cda_power, cda_toughness)) => {
                                if power {
                                    cda_power
                                } else {
                                    cda_toughness
                                }
                            }
                            None => printed.parse::<i32>().map_err(|_| {
                                EngineError::new(
                                    "an object condition over a non-numeric printed power or toughness without a characteristic-defining ability",
                                )
                            })?,
                        }
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
