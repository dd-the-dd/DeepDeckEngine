//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("copyResolvingSpellIfCastFromZone") => {
            effect["zone"].is_string()
                && effect["optional"].is_boolean()
                && effect["chooseNewTargets"].is_boolean()
        }
        Some("copyStackItem") => {
            target_reference_supported(&effect["object"])
                && player_reference_supported(&effect["controller"])
                && effect
                    .get("mayChooseNewTargets")
                    .is_none_or(Value::is_boolean)
                && effect.get("removeLegendary").is_none_or(Value::is_boolean)
                // CR 707.10: the copy's copiable-value exceptions, the same
                // modification list a token copy applies (CR 707.9b).
                && effect.get("except").is_none_or(|exceptions| {
                    exceptions.as_array().is_some_and(|exceptions| {
                        exceptions
                            .iter()
                            .all(super::super::validation::copy_exception_supported)
                    })
                })
                && effect.get("grantKeywords").is_none_or(|keywords| {
                    keywords.as_array().is_some_and(|keywords| {
                        keywords.iter().all(|keyword| {
                            keyword.as_str().is_some_and(|keyword| {
                                keyword_kind_supported(&json!({ "kind": keyword }))
                            })
                        })
                    })
                })
        }
        Some("copyResolvingStackObject") => {
            player_reference_supported(&effect["controller"])
                && effect
                    .get("mayChooseNewTargets")
                    .is_none_or(Value::is_boolean)
        }
        _ => false,
    }
}

impl GameEngine {
    /// CR 707.10: a copy of a spell or ability copies the copiable values
    /// of the original, then applies the effect's exceptions to them.
    fn apply_stack_copy_exceptions(
        &mut self,
        copy_id: &str,
        exceptions: &[Value],
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let Some(mut definition) = self
            .state
            .stack
            .iter()
            .find(|object| object.id == copy_id)
            .map(|copy| copy.card.definition.clone())
        else {
            return Ok(());
        };
        for exception in exceptions {
            self.apply_copy_exception(
                &mut definition,
                exception,
                stack_object,
                bindings,
                decisions,
            )?;
        }
        if let Some(copy) = self
            .state
            .stack
            .iter_mut()
            .find(|object| object.id == copy_id)
        {
            copy.card.definition = definition;
        }
        Ok(())
    }

    pub(super) fn execute_stack_copying_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_stack_copying_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_stack_copying_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.check_quantity_failure()?;
        match value_kind(effect) {
            Some("copyResolvingSpellIfCastFromZone") => {
                if is_spell_copy(&stack_object.card)
                    || stack_object.decisions["castSourceZone"].as_str() != effect["zone"].as_str()
                {
                    return Ok(());
                }
                if effect["optional"].as_bool().unwrap_or(false)
                    && !self.confirm_resolution_action(
                        stack_object,
                        provider,
                        &stack_object.controller,
                        "Copy this spell",
                    )?
                {
                    return Ok(());
                }
                self.copy_sequence += 1;
                self.stack_sequence += 1;
                let copy_stack_id = format!("stack:{}", self.stack_sequence);
                let mut card = stack_object.card.clone();
                card.instance_id =
                    format!("copy:spell:{}:{}", card.instance_id, self.copy_sequence);
                card.flags.insert("isSpellCopy".to_string(), true);
                card.flags.remove("wasCast");
                self.state.stack.push(StackObject {
                    id: copy_stack_id.clone(),
                    controller: stack_object.controller.clone(),
                    card,
                    cant_be_countered: counter_protection::copiable_stack_counter_protection(
                        stack_object,
                    ),
                    exile_on_leave_stack: false,
                    ability_kind: None,
                    ability_rule: None,
                    decisions: stack_object.decisions.clone(),
                    targets: stack_object.targets.clone(),
                });
                self.record_event(
                    "stackObjectCopied",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({
                        "cardName": stack_object.card.definition.name,
                        "objectKind": "stackCopy",
                        "resolvingSpell": true,
                        "stackId": copy_stack_id,
                    }),
                );
                if effect["chooseNewTargets"].as_bool().unwrap_or(false) {
                    self.choose_new_targets_for_copy(
                        &copy_stack_id,
                        &stack_object.controller,
                        provider,
                    )?;
                }
            }
            Some("copyStackItem") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    let granted_keywords = effect["grantKeywords"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    let Some(copy_id) =
                        self.copy_stack_item_from_effect(&stack_id, effect, stack_object)
                    else {
                        return Ok(());
                    };
                    self.check_quantity_failure()?;
                    let grant_keywords = !granted_keywords.is_empty()
                        && (effect["grantKeywordsIfPermanentSpell"].as_bool() != Some(true)
                            || self
                                .state
                                .stack
                                .iter()
                                .find(|object| object.id == copy_id)
                                .is_some_and(|copy| is_permanent(&copy.card.definition)));
                    if grant_keywords
                        && let Some(copy) = self
                            .state
                            .stack
                            .iter_mut()
                            .find(|object| object.id == copy_id)
                    {
                        for keyword in &granted_keywords {
                            copy.card.definition.printed_rules_mut().push(json!({
                                "kind": "keywordAbility",
                                "source": { "kind": "self" },
                                "ability": { "kind": keyword },
                            }));
                        }
                    }
                    if let Some(controller_id) =
                        self.resolve_player(&effect["controller"], stack_object, bindings)
                        && let Some(copy) = self
                            .state
                            .stack
                            .iter_mut()
                            .find(|object| object.id == copy_id)
                    {
                        copy.controller = controller_id.clone();
                        copy.card.controller = controller_id;
                    }
                    if effect["removeLegendary"].as_bool() == Some(true)
                        && let Some(copy) = self
                            .state
                            .stack
                            .iter_mut()
                            .find(|object| object.id == copy_id)
                    {
                        copy.card.definition.type_line = copy
                            .card
                            .definition
                            .type_line
                            .strip_prefix("Legendary ")
                            .unwrap_or(&copy.card.definition.type_line)
                            .to_string();
                    }
                    if let Some(exceptions) = effect["except"].as_array() {
                        self.apply_stack_copy_exceptions(
                            &copy_id,
                            exceptions,
                            stack_object,
                            bindings,
                            decisions,
                        )?;
                    }
                    if effect["mayChooseNewTargets"].as_bool() == Some(true) {
                        let chooser = self
                            .resolve_player(&effect["controller"], stack_object, bindings)
                            .unwrap_or_else(|| stack_object.controller.clone());
                        self.choose_new_targets_for_copy(&copy_id, &chooser, provider)?;
                    }
                }
            }
            Some("copyResolvingStackObject") => {
                let copy_id = self.copy_stack_object_value(stack_object, &stack_object.id);
                self.check_quantity_failure()?;
                if let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                {
                    if let Some(copy) = self
                        .state
                        .stack
                        .iter_mut()
                        .find(|object| object.id == copy_id)
                    {
                        copy.controller = controller_id.clone();
                        copy.card.controller = controller_id.clone();
                    }
                    if effect["mayChooseNewTargets"].as_bool() == Some(true) {
                        self.choose_new_targets_for_copy(&copy_id, &controller_id, provider)?;
                    }
                }
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(())
    }
}
