//! Joint final-set evaluation reuses the declared slot and constraint service.
use super::*;
use crate::engine::{EngineError, target_reassignment::PreparedTargetReassignment};

const MAX_EXPLORED_ASSIGNMENTS: usize = 16_384;

fn selection_for(stack: &StackObject, id: &str) -> Vec<TargetRef> {
    let mut selected = stack
        .targets
        .iter()
        .filter_map(|(key, target)| {
            let slots = declared_slots(stack, key);
            (slots.len() == 1 && slots[0].declaration_id == id)
                .then(|| {
                    target_ordinal(id, key).map(|ordinal| (ordinal.unwrap_or(0), target.clone()))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|(ordinal, _)| *ordinal);
    selected.into_iter().map(|(_, target)| target).collect()
}

fn update_target_decisions(
    stack: &mut StackObject,
    order: &[(String, String)],
) -> Result<(), EngineError> {
    let mut seen = Vec::<&str>::new();
    for (_, declaration) in order {
        if seen.contains(&declaration.as_str()) {
            continue;
        }
        seen.push(declaration);
        let selection = selection_for(stack, declaration);
        stack.decisions.insert(
            declaration.clone(),
            serde_json::to_value(selection)
                .map_err(|_| EngineError::new("target selection cannot be serialized"))?,
        );
    }
    Ok(())
}

fn references(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            let id = match value_kind(value) {
                Some("chosenTarget" | "chosenTargets") => value["id"].as_str(),
                Some("decisionResult") => value["decisionId"].as_str(),
                _ => None,
            };
            if let Some(id) = value.get("targetId").and_then(Value::as_str) {
                out.push(id.to_string());
            }
            if let Some(id) = id {
                out.push(id.to_string());
            }
            for child in fields.values() {
                references(child, out);
            }
        }
        Value::Array(children) => {
            for child in children {
                references(child, out);
            }
        }
        _ => {}
    }
}

fn source_dependent(value: &Value) -> bool {
    if value_kind(value) == Some("controllerOf")
        && matches!(value_kind(&value["object"]), Some("self" | "abilitySource"))
    {
        return false;
    }
    match value {
        Value::Object(fields) => {
            value["excludeSource"].as_bool() == Some(true)
                || matches!(
                    value_kind(value),
                    Some("self" | "abilitySource" | "attachedPermanent")
                )
                || fields.values().any(source_dependent)
        }
        Value::Array(children) => children.iter().any(source_dependent),
        _ => false,
    }
}

impl GameEngine {
    fn ordered_joint_target_slots(
        &self,
        stack: &StackObject,
    ) -> Result<Vec<(String, String)>, EngineError> {
        let mut result = Vec::new();
        let mut declaration_order = Vec::<String>::new();
        for rule in declaration_rules(stack) {
            for decision in rule["declaration"]["decisions"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if value_kind(decision) != Some("chooseTargets") {
                    continue;
                }
                let id = decision["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("target declaration id missing"))?;
                if declaration_order.iter().any(|old| old == id) {
                    return Err(EngineError::new("ambiguous target declaration id"));
                }
                if source_dependent(&decision["candidates"]) {
                    let source_marker = Self::tracked_zone_identity(&stack.card)
                        .filter(|marker| !marker.is_empty());
                    let authenticated_live = !crate::engine::source_lki::source_has_departed(stack)
                        && source_marker.is_some()
                        && self
                            .permanent_position(&stack.card.instance_id)
                            .is_some_and(|(p, c)| {
                                Self::tracked_zone_identity(&self.state.players[p].battlefield[c])
                                    == source_marker
                            });
                    if !authenticated_live {
                        return Err(EngineError::new(
                            "joint target source-dependent candidate needs an authenticated live source context",
                        ));
                    }
                }
                let mut refs = Vec::new();
                references(&decision["candidates"], &mut refs);
                for reference in refs {
                    let referenced = declared_slots(stack, &reference);
                    let parent = if referenced.len() == 1 {
                        Some(referenced[0].declaration_id)
                    } else {
                        declaration_rules(stack)
                            .into_iter()
                            .flat_map(|r| {
                                r["declaration"]["decisions"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                            })
                            .find(|d| {
                                value_kind(d) == Some("chooseTargets")
                                    && d["id"].as_str() == Some(reference.as_str())
                            })
                            .and_then(|d| d["id"].as_str())
                    };
                    if let Some(parent) = parent {
                        if !declaration_order.iter().any(|prior| prior == parent) {
                            return Err(EngineError::new(
                                "joint target declaration has a forward or cyclic dependency",
                            ));
                        }
                    } else if !stack.decisions.contains_key(&reference) {
                        return Err(EngineError::new(
                            "joint target candidate has an unavailable declaration binding",
                        ));
                    }
                }
                declaration_order.push(id.to_string());
                let mut keys = stack
                    .targets
                    .keys()
                    .filter_map(|key| {
                        let slots = declared_slots(stack, key);
                        (slots.len() == 1 && slots[0].declaration_id == id)
                            .then(|| {
                                target_ordinal(id, key)
                                    .map(|ordinal| (ordinal.unwrap_or(0), key.clone()))
                            })
                            .flatten()
                    })
                    .collect::<Vec<_>>();
                keys.sort_by_key(|(ordinal, _)| *ordinal);
                if keys.windows(2).any(|pair| pair[0].0 == pair[1].0) {
                    return Err(EngineError::new(
                        "joint target slots have an ambiguous ordinal",
                    ));
                }
                result.extend(keys.into_iter().map(|(_, key)| (key, id.to_string())));
            }
        }
        if result.len() != stack.targets.len() {
            return Err(EngineError::new(
                "joint target reassignment requires explicit unambiguous target slots",
            ));
        }
        Ok(result)
    }

    fn same_joint_identity(
        &self,
        stack: &StackObject,
        left: &str,
        right: &str,
    ) -> Result<bool, EngineError> {
        if stack.targets.get(left) != stack.targets.get(right) {
            return Ok(false);
        }
        if matches!(stack.targets.get(left), Some(TargetRef::Permanent { .. })) {
            let a = Self::captured_target_numeric_binding(stack, left)
                .map_err(|_| EngineError::new("joint target identity is unavailable"))?;
            let b = Self::captured_target_numeric_binding(stack, right)
                .map_err(|_| EngineError::new("joint target identity is unavailable"))?;
            return Ok(a.identity == b.identity);
        }
        Ok(true)
    }

    /// Check constraints ONLY against the completed hypothetical assignment.
    fn joint_target_constraints(
        &self,
        original: &StackObject,
        stack: &StackObject,
        replaced: &[String],
    ) -> Result<bool, EngineError> {
        let assignment = CastingDeclaration {
            decisions: stack.decisions.clone(),
            targets: stack.targets.clone(),
            target_order: self
                .ordered_joint_target_slots(stack)?
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
        };
        for rule in declaration_rules(stack) {
            for decision in rule["declaration"]["decisions"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if value_kind(decision) != Some("chooseTargets") {
                    continue;
                }
                let id = decision["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("target declaration id missing"))?;
                let keys = assignment
                    .target_order
                    .iter()
                    .filter(|key| {
                        let slots = declared_slots(stack, key);
                        slots.len() == 1 && slots[0].declaration_id == id
                    })
                    .collect::<Vec<_>>();
                for (index, key) in keys.iter().enumerate() {
                    for prior in &keys[..index] {
                        if self.same_joint_identity(stack, key, prior)? {
                            return Ok(false);
                        }
                    }
                }
                if let Some(constraint) = decision.get("selectionConstraint") {
                    let mut refs = Vec::new();
                    references(constraint, &mut refs);
                    let affected = keys.iter().any(|key| replaced.contains(key))
                        || refs.iter().any(|reference| replaced.contains(reference));
                    if !affected {
                        continue;
                    }
                    if keys.iter().any(|key| {
                        !replaced.contains(key)
                            && !self.target_is_legal_for_stack_object(
                                key,
                                &original.targets[*key],
                                original,
                            )
                    }) {
                        return Err(EngineError::new(
                            "joint selection constraint needs unavailable retained-target characteristics",
                        ));
                    }
                    if !self.target_selection_satisfies_constraint(
                        &selection_for(stack, id),
                        constraint,
                        &assignment,
                    ) {
                        return Ok(false);
                    }
                }
            }
            for group in rule["distinctTargets"].as_array().into_iter().flatten() {
                let keys = group
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|id| stack.targets.contains_key(*id))
                    .collect::<Vec<_>>();
                if !keys
                    .iter()
                    .any(|id| replaced.iter().any(|changed| changed == id))
                {
                    continue;
                }
                for (index, key) in keys.iter().enumerate() {
                    for prior in &keys[..index] {
                        if self.same_joint_identity(stack, key, prior)? {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    fn joint_assignment_is_valid(
        &self,
        original: &StackObject,
        prepared: &PreparedTargetReassignment,
    ) -> Result<bool, EngineError> {
        let order = self.ordered_joint_target_slots(original)?;
        if prepared.target_order != order.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>()
            || prepared.targets.len() != original.targets.len()
            || original.targets.keys().ne(prepared.targets.keys())
        {
            return Err(EngineError::new(
                "joint reassignment changed target count or order",
            ));
        }
        let mut final_stack = original.clone();
        final_stack.targets = prepared.targets.clone();
        final_stack.decisions = prepared.decisions.clone();
        for id in &prepared.replaced {
            let target = final_stack
                .targets
                .get(id)
                .ok_or_else(|| EngineError::new("unknown reassignment slot"))?;
            if matches!(target,TargetRef::StackObject{stack_id} if stack_id==&original.id)
                || !self.target_is_legal_for_stack_object(id, target, &final_stack)
            {
                return Ok(false);
            }
        }
        if prepared.replaced.is_empty() {
            return Ok(true);
        }
        if !self.joint_target_constraints(original, &final_stack, &prepared.replaced)? {
            return Ok(false);
        }
        // Retaining an already illegal target is permitted. A replacement may
        // not make a previously legal retained target illegal (CR115.7d).
        for (id, target) in &original.targets {
            if !prepared.replaced.contains(id)
                && self.target_is_legal_for_stack_object(id, target, original)
                && !self.target_is_legal_for_stack_object(id, target, &final_stack)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(in crate::engine) fn validate_prepared_joint_targets(
        &self,
        original: &StackObject,
        prepared: &PreparedTargetReassignment,
    ) -> Result<(), EngineError> {
        if self.joint_assignment_is_valid(original, prepared)? {
            Ok(())
        } else {
            Err(EngineError::new("invalid final joint target assignment"))
        }
    }

    pub(in crate::engine) fn prepare_joint_target_assignments(
        &self,
        original: &StackObject,
    ) -> Result<Vec<PreparedTargetReassignment>, EngineError> {
        // Authentication happens before enumeration/provider, never after a
        // user has selected an option. Nonbattlefield card incarnations do not
        // yet have the accepted battlefield capture contract.
        for (id, target) in &original.targets {
            match target {
                TargetRef::Permanent { .. } => {
                    Self::captured_target_numeric_binding(original,id)
                    .map_err(|_|EngineError::new("joint reassignment requires authenticated original battlefield targets"))?;
                }
                TargetRef::Card { .. } => {
                    return Err(EngineError::new(
                        "joint reassignment of nonbattlefield card incarnations is unsupported",
                    ));
                }
                _ => {}
            }
        }
        let order = self.ordered_joint_target_slots(original)?;
        if order.len() > 64 {
            return Err(EngineError::new(
                "joint target assignment depth limit exceeded",
            ));
        }
        let mut current = original.clone();
        for (_, id) in &order {
            let expected = serde_json::to_value(selection_for(original, id))
                .map_err(|_| EngineError::new("target selection cannot be serialized"))?;
            if original.decisions.get(id) != Some(&expected) {
                return Err(EngineError::new(
                    "joint reassignment requires the committed target decision snapshot",
                ));
            }
        }
        let mut results = Vec::new();
        let mut explored = 0usize;
        let mut replaced = Vec::new();
        self.collect_joint_target_assignments(
            original,
            &mut current,
            &order,
            0,
            &mut replaced,
            &mut explored,
            &mut results,
        )?;
        Ok(results)
    }

    fn collect_joint_target_assignments(
        &self,
        original: &StackObject,
        current: &mut StackObject,
        order: &[(String, String)],
        index: usize,
        replaced: &mut Vec<String>,
        explored: &mut usize,
        results: &mut Vec<PreparedTargetReassignment>,
    ) -> Result<(), EngineError> {
        *explored += 1;
        if *explored > MAX_EXPLORED_ASSIGNMENTS {
            return Err(EngineError::new(
                "joint target assignment exploration limit reached before completion",
            ));
        }
        if index == order.len() {
            let prepared = PreparedTargetReassignment {
                target_order: order.iter().map(|(id, _)| id.clone()).collect(),
                targets: current.targets.clone(),
                decisions: current.decisions.clone(),
                replaced: replaced.clone(),
            };
            if self.joint_assignment_is_valid(original, &prepared)? {
                results.push(prepared);
            }
            return Ok(());
        }
        let id = &order[index].0;
        let before = current.clone();
        // Retain keeps the original capture/LKI, including an old incarnation.
        self.collect_joint_target_assignments(
            original,
            current,
            order,
            index + 1,
            replaced,
            explored,
            results,
        )?;
        *current = before.clone();
        let candidates = self
            .declared_stack_target_candidates(current, id)
            .ok_or_else(|| EngineError::new("joint target candidate context is unsupported"))?;
        if candidates
            .iter()
            .any(|candidate| matches!(candidate, TargetRef::Card { .. }))
        {
            return Err(EngineError::new(
                "joint reassignment of nonbattlefield card incarnations is unsupported",
            ));
        }
        for candidate in candidates {
            if original.targets.get(id) == Some(&candidate)
                && self.target_incarnation_is_current(original, id, &candidate)
            {
                continue;
            }
            current.targets.insert(id.clone(), candidate.clone());
            self.capture_target_incarnation(&mut current.decisions, id, &candidate)?;
            if matches!(candidate, TargetRef::Permanent { .. }) {
                Self::captured_target_numeric_binding(current, id).map_err(|_| {
                    EngineError::new(
                        "joint replacement target lacks authenticated battlefield identity",
                    )
                })?;
            }
            update_target_decisions(current, order)?;
            replaced.push(id.clone());
            self.collect_joint_target_assignments(
                original,
                current,
                order,
                index + 1,
                replaced,
                explored,
                results,
            )?;
            replaced.pop();
            *current = before.clone();
        }
        Ok(())
    }
}
