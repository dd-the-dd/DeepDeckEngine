//! Engine adapter for authenticated target numeric reads and departure capture.
//! The target designation module owns metadata; this module owns live/LKI choice.
use super::target_numeric_lki::{
    BattlefieldTargetIdentity, NumericTargetBinding, PreparedTargetDeparture, TargetNumericContext,
    TargetNumericProperty, TargetNumericReadError, TargetNumericValues, read_target_numeric,
};
use super::{
    CardInstance, EngineError, GameEngine, StackObject, TargetRef, mana_value, value_kind,
};
use serde_json::Value;
use std::collections::BTreeMap;

impl GameEngine {
    fn unauthenticated_resolution_target_ids(
        &self,
        stack: &StackObject,
    ) -> std::collections::BTreeSet<String> {
        stack
            .targets
            .iter()
            .filter_map(|(id, target)| {
                if matches!(target, TargetRef::Card { .. }) {
                    return self
                        .validate_card_target_authentication(stack, id)
                        .is_err()
                        .then_some(id.clone());
                }
                let TargetRef::Permanent { instance_id } = target else {
                    return None;
                };
                let capture_invalid = Self::captured_target_numeric_binding(stack, id).is_err();
                let live_invalid =
                    self.permanent_position(instance_id)
                        .is_some_and(|(player, index)| {
                            super::object_identity::object_identity_of(
                                &self.state.players[player].battlefield[index],
                            )
                            .is_err()
                        });
                (capture_invalid || live_invalid).then_some(id.clone())
            })
            .collect()
    }

    /// Called read-only before pop. No designation may be minted/repaired here.
    pub(super) fn validate_resolution_target_authentication(
        &self,
        stack: &StackObject,
    ) -> Result<(), EngineError> {
        let missing = self.unauthenticated_resolution_target_ids(stack);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(EngineError::new(format!(
                "unsupported target authentication context: {}",
                missing.into_iter().collect::<Vec<_>>().join(",")
            )))
        }
    }

    pub(super) fn target_numeric_context(&self, stack: &StackObject) -> TargetNumericContext {
        let targets = Self::captured_target_numeric_bindings(stack);
        let legal_at_start = stack
            .targets
            .keys()
            .filter(|id| {
                stack
                    .targets
                    .get(*id)
                    .is_some_and(|target| self.target_is_legal_for_stack_object(id, target, stack))
            })
            .cloned()
            .collect();
        let unselected_at_start = super::target_legality::declared_unselected_target_ids(stack);
        let mut known_targets_at_start = stack
            .targets
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        known_targets_at_start.extend(unselected_at_start.iter().cloned());
        let unauthenticated_at_start = self.unauthenticated_resolution_target_ids(stack);
        TargetNumericContext {
            stack_id: stack.id.clone(),
            targets,
            legal_at_start,
            known_targets_at_start,
            unselected_at_start,
            unauthenticated_at_start,
        }
    }

    pub(super) fn with_target_numeric_context<T>(
        &mut self,
        context: TargetNumericContext,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        if !context.unauthenticated_at_start.is_empty() {
            return Err(EngineError::new(
                "unsupported target authentication context",
            ));
        }
        if context.targets.is_empty() && context.known_targets_at_start.is_empty() {
            return work(self);
        }
        let scope = self
            .target_numeric_contexts
            .begin(context)
            .map_err(|error| EngineError::new(format!("target numeric scope: {error:?}")))?;
        let result = work(self);
        let cleanup = self
            .target_numeric_contexts
            .end(scope)
            .map_err(|error| EngineError::new(format!("target numeric scope cleanup: {error:?}")));
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    }

    fn effective_target_numeric_values(
        &self,
        card: &CardInstance,
        requested: Option<TargetNumericProperty>,
    ) -> TargetNumericValues {
        let creature = self.permanent_is_creature(card);
        TargetNumericValues {
            mana_value: mana_value(&card.definition),
            power: (creature
                && requested.is_none_or(|property| property == TargetNumericProperty::Power))
            .then(|| self.current_power(card)),
            toughness: (creature
                && requested.is_none_or(|property| property == TargetNumericProperty::Toughness))
            .then(|| self.current_toughness(card)),
        }
    }

    pub(super) fn prepare_target_numeric_departure(
        &self,
        instance_id: &str,
    ) -> Option<PreparedTargetDeparture> {
        let (player, index) = self.permanent_position(instance_id)?;
        let card = &self.state.players[player].battlefield[index];
        let identity = BattlefieldTargetIdentity {
            instance_id: card.instance_id.clone(),
            zone_identity: Self::tracked_zone_identity(card)?.to_string(),
        };
        let observed = self.target_numeric_contexts.observes(&identity)
            || self.state.stack.iter().any(|stack| {
                Self::captured_target_numeric_bindings(stack)
                    .values()
                    .any(|binding| binding.identity == identity)
            });
        observed.then(|| PreparedTargetDeparture {
            identity,
            values: self.effective_target_numeric_values(card, None),
        })
    }

    pub(super) fn confirm_target_numeric_departure(
        &mut self,
        departure: Option<PreparedTargetDeparture>,
    ) {
        let Some(departure) = departure else {
            return;
        };
        self.target_numeric_contexts.confirm_departure(&departure);
        Self::capture_pending_target_numeric_departure(&mut self.state.stack, &departure);
    }

    pub(super) fn target_numeric_departure_for_install(
        &self,
        instance_id: &str,
        prepared: Option<PreparedTargetDeparture>,
    ) -> Result<Option<PreparedTargetDeparture>, EngineError> {
        let Some(prepared) = prepared else {
            return Ok(self.prepare_target_numeric_departure(instance_id));
        };
        let same = self
            .permanent_position(instance_id)
            .is_some_and(|(player, index)| {
                let card = &self.state.players[player].battlefield[index];
                prepared.identity.instance_id == card.instance_id
                    && Self::tracked_zone_identity(card)
                        == Some(prepared.identity.zone_identity.as_str())
            });
        if !same {
            return Err(EngineError::new(
                "planned target numeric snapshot no longer identifies the departing object",
            ));
        }
        Ok(Some(prepared))
    }

    /// Only scalar chosenTarget/Permanent is in this initial capability.
    /// The caller retains its existing handling for card/stack/player targets.
    pub(super) fn read_chosen_target_numeric(
        &self,
        property: TargetNumericProperty,
        stack: &StackObject,
        target_id: &str,
    ) -> Result<i32, TargetNumericReadError> {
        if self.property_target_illegal_at_start(stack, target_id) {
            return Err(TargetNumericReadError::IllegalAtResolutionStart);
        }
        let original: NumericTargetBinding =
            Self::captured_target_numeric_binding(stack, target_id)?;
        let active = self.target_numeric_contexts.binding(&stack.id, target_id);
        let binding = active.unwrap_or(&original);
        if binding.identity != original.identity {
            return Err(TargetNumericReadError::ConflictingContext);
        }
        let TargetRef::Permanent { instance_id } = stack
            .targets
            .get(target_id)
            .ok_or(TargetNumericReadError::MissingDesignation)?
        else {
            return Err(TargetNumericReadError::UnsupportedTargetKind);
        };
        let live = if let Some((player, index)) = self.permanent_position(instance_id) {
            let card = &self.state.players[player].battlefield[index];
            let identity = super::object_identity::object_identity_of(card)
                .map_err(|_| TargetNumericReadError::MissingIncarnation)?;
            // A returned valid incarnation is not the original target. A live
            // object without authentication is not an authenticated departure.
            (identity == binding.identity).then(|| {
                (
                    identity,
                    self.effective_target_numeric_values(card, Some(property)),
                )
            })
        } else {
            None
        };
        if live.is_none() && active.is_none() {
            return Err(TargetNumericReadError::MissingResolutionContext);
        }
        read_target_numeric(
            binding,
            live.as_ref().map(|(identity, values)| (identity, values)),
            property,
        )
    }
    pub(super) fn target_was_legal_at_resolution_start(
        &self,
        stack: &StackObject,
        id: &str,
    ) -> Option<bool> {
        self.target_numeric_contexts.legal_at_start(&stack.id, id)
    }

    /// Scalar subject callers need no condition-evaluation context.
    pub(super) fn value_requires_initially_illegal_target(
        &self,
        value: &Value,
        stack: &StackObject,
    ) -> Result<bool, EngineError> {
        self.inspect_initial_target_dependencies(value, stack, None)
    }

    pub(super) fn value_requires_initially_illegal_target_in_context(
        &self,
        value: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, super::RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        self.inspect_initial_target_dependencies(value, stack, Some((bindings, decisions)))
    }

    /// Inspect only evaluated arguments, never a quoted future program.
    /// The numeric interpreter's selected branch owns conditionalValue.
    fn inspect_initial_target_dependencies(
        &self,
        value: &Value,
        stack: &StackObject,
        context: Option<(
            &BTreeMap<String, super::RuntimeBinding>,
            &BTreeMap<String, Value>,
        )>,
    ) -> Result<bool, EngineError> {
        fn visit(
            engine: &GameEngine,
            value: &Value,
            stack: &StackObject,
            context: Option<(
                &BTreeMap<String, super::RuntimeBinding>,
                &BTreeMap<String, Value>,
            )>,
            depth: usize,
        ) -> Result<bool, EngineError> {
            if depth >= 64 {
                return Err(EngineError::new(
                    "target reference nesting exceeds supported depth",
                ));
            }
            if value_kind(value) == Some("conditionalValue") {
                let (bindings, decisions) = context.ok_or_else(|| {
                    EngineError::new(
                        "conditional target dependency requires numeric decision context",
                    )
                })?;
                if engine.condition_requires_initially_illegal_target(
                    &value["condition"],
                    stack,
                    bindings,
                    decisions,
                    depth + 1,
                )? {
                    return Ok(true);
                }
                // Same condition service and laziness as checked_runtime_numeric_expression.
                let branch = if engine.checked_runtime_numeric_condition(
                    &value["condition"],
                    stack,
                    bindings,
                    decisions,
                )? {
                    &value["ifTrue"]
                } else {
                    &value["ifFalse"]
                };
                return visit(engine, branch, stack, context, depth + 1);
            }
            if value_kind(value) == Some("chosenTarget") {
                let id = value["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("missing target reference identifier"))?;
                return engine
                    .target_was_legal_at_resolution_start(stack, id)
                    .map(|legal| !legal)
                    .ok_or_else(|| {
                        EngineError::new(format!(
                            "missing resolution-start target legality for `{id}`"
                        ))
                    });
            }
            match value {
                Value::Object(fields) => {
                    for child in fields.values() {
                        if visit(engine, child, stack, context, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
                Value::Array(items) => {
                    for child in items {
                        if visit(engine, child, stack, context, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
                _ => {}
            }
            Ok(false)
        }
        visit(self, value, stack, context, 0)
    }

    /// Dependency preparation follows the existing condition service's lazy
    /// Boolean evaluation; it does not decide truth or invent numeric values.
    pub(super) fn condition_requires_initially_illegal_target(
        &self,
        condition: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, super::RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        depth: usize,
    ) -> Result<bool, EngineError> {
        if depth >= 64 {
            return Err(EngineError::new(
                "target condition nesting exceeds supported depth",
            ));
        }
        match value_kind(condition) {
            Some(kind @ ("and" | "or")) => {
                let operands = condition["operands"]
                    .as_array()
                    .ok_or_else(|| EngineError::new("missing logical condition operands"))?;
                for operand in operands {
                    if self.condition_requires_initially_illegal_target(
                        operand,
                        stack,
                        bindings,
                        decisions,
                        depth + 1,
                    )? {
                        return Ok(true);
                    }
                    let truth = self
                        .checked_runtime_numeric_condition(operand, stack, bindings, decisions)?;
                    if (kind == "and" && !truth) || (kind == "or" && truth) {
                        break;
                    }
                }
                Ok(false)
            }
            Some("not") => self.condition_requires_initially_illegal_target(
                &condition["operand"],
                stack,
                bindings,
                decisions,
                depth + 1,
            ),
            Some("compare") => Ok(self.numeric_amount_requires_initially_illegal_target(
                &condition["left"],
                stack,
                bindings,
                decisions,
            )? || self.numeric_amount_requires_initially_illegal_target(
                &condition["right"],
                stack,
                bindings,
                decisions,
            )?),
            _ => self.value_requires_initially_illegal_target_in_context(
                condition, stack, bindings, decisions,
            ),
        }
    }

    fn property_target_illegal_at_start(&self, stack: &StackObject, id: &str) -> bool {
        match self.target_numeric_contexts.legal_at_start(&stack.id, id) {
            Some(legal) => !legal,
            // Without a frame there is no proved resolution-start state.
            // Unknown context is never reclassified as a rules-defined skip.
            None => false,
        }
    }

    /// CR608.2b is distinct from missing runtime information: a part needing
    /// information about an initially illegal target does not happen. This
    /// walks validated numeric IR, without evaluating/reordering arithmetic.
    pub(super) fn numeric_amount_requires_initially_illegal_target(
        &self,
        amount: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, super::RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(amount) == Some("conditionalValue") {
            if self.value_requires_initially_illegal_target_in_context(
                amount, stack, bindings, decisions,
            )? {
                return Ok(true);
            }
            let branch = if self.checked_runtime_numeric_condition(
                &amount["condition"],
                stack,
                bindings,
                decisions,
            )? {
                &amount["ifTrue"]
            } else {
                &amount["ifFalse"]
            };
            return self.numeric_amount_requires_initially_illegal_target(
                branch, stack, bindings, decisions,
            );
        }
        if value_kind(amount) == Some("countObjects") {
            return Ok(!matches!(
                self.resolve_runtime_collection_status(
                    &amount["objects"],
                    stack,
                    bindings,
                    decisions
                )?,
                super::runtime_collections::RuntimeCollectionResolution::Complete(_)
            ));
        }
        if matches!(
            value_kind(amount),
            Some("manaValueOf" | "powerOf" | "toughnessOf")
        ) && value_kind(&amount["object"]) == Some("chosenTarget")
            && let Some(id) = amount["object"]["id"].as_str()
        {
            return Ok(self.property_target_illegal_at_start(stack, id));
        }
        if value_kind(amount) == Some("chosenTarget") {
            return self.value_requires_initially_illegal_target(amount, stack);
        }
        match amount {
            Value::Array(children) => {
                for child in children {
                    if self.numeric_amount_requires_initially_illegal_target(
                        child, stack, bindings, decisions,
                    )? {
                        return Ok(true);
                    }
                }
            }
            Value::Object(children) => {
                for child in children.values() {
                    if self.numeric_amount_requires_initially_illegal_target(
                        child, stack, bindings, decisions,
                    )? {
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "target_numeric_runtime_tests.rs"]
mod tests;
