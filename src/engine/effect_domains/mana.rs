//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("addMana") => {
            player_reference_supported(&effect["player"])
                && mana_value_supported(&effect["mana"])
                && effect
                    .get("spendRestriction")
                    .is_none_or(mana_spend_restriction_supported)
        }
        Some("addFixedManaSymbols") => {
            player_reference_supported(&effect["player"])
                && effect["symbols"].as_array().is_some_and(|symbols| {
                    !symbols.is_empty()
                        && symbols.iter().all(|symbol| {
                            matches!(symbol.as_str(), Some("W" | "U" | "B" | "R" | "G" | "C"))
                        })
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_mana_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_mana_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_mana_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("addMana") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let outputs = self.try_runtime_mana_options(
                    &effect["mana"],
                    &player_id,
                    stack_object,
                    bindings,
                    decisions,
                )?;
                let options = match outputs {
                    mana_projection_context::ExecutableManaOutputs::NoMana(_) => return Ok(()),
                    mana_projection_context::ExecutableManaOutputs::Choices(options) => options,
                };
                let symbols = if options.len() <= 1 {
                    let Some(only) = options.into_iter().next() else {
                        return Err(EngineError::new(
                            "mana.execution.empty-choice-set: executable output invariant failed",
                        ));
                    };
                    only
                } else {
                    let actions = options
                        .iter()
                        .enumerate()
                        .map(|(index, symbols)| LegalAction {
                            id: format!("resolve:{}:mana:{index}", stack_object.id),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!(
                                "Add {}",
                                symbols
                                    .iter()
                                    .map(|symbol| format!("{{{symbol}}}"))
                                    .collect::<String>()
                            ),
                            card_instance_id: Some(stack_object.card.instance_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::from([("manaChoice".to_string(), json!(symbols))]),
                            targets: BTreeMap::new(),
                            target_order: Vec::new(),
                            attacker_id: None,
                            blocker_id: None,
                        })
                        .collect::<Vec<_>>();
                    self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("resolution:{}:mana", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id: player_id.clone(),
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options: actions,
                        },
                    )?
                    .decisions["manaChoice"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                };
                if symbols.is_empty() {
                    // A selected zero-output branch resolves without producing
                    // mana. Other instructions/ability signals still continue.
                    return Ok(());
                }
                let player_index = self.player_index(&player_id)?;
                let spend_restriction = effect
                    .get("spendRestriction")
                    .or_else(|| effect["mana"].get("spendRestriction"))
                    .cloned();
                for symbol in &symbols {
                    self.state.players[player_index]
                        .mana_pool
                        .push(FloatingMana {
                            symbol: symbol.clone(),
                            spend_restriction: spend_restriction.clone(),
                        });
                }
                self.record_event(
                    "manaAdded",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "mana": symbols }),
                );
            }
            Some("addFixedManaSymbols") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let symbols = effect["symbols"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                let player_index = self.player_index(&player_id)?;
                for symbol in &symbols {
                    self.state.players[player_index]
                        .mana_pool
                        .push(FloatingMana {
                            symbol: symbol.clone(),
                            spend_restriction: None,
                        });
                }
                self.record_event(
                    "manaAdded",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "mana": symbols }),
                );
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
