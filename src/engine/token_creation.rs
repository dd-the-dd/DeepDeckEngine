//! Token quantity preparation and simultaneous battlefield installation.
//! Tokens are transformed before outer entry choices; contained counters are
//! prepared independently for each resulting object (CR616.1g).

use super::quantity_replacement::{QuantityEvent, QuantityOrigin, QuantityRecipient};
use super::*;

/// Only bounded literals are infallible quantities. Other structured reads
/// and arithmetic need the existing outer checkpoint before earlier actions.
/// This is shallow: resolution_preflight owns the only recursive visitor.
pub(super) fn node_requires_checkpoint(effect: &Value) -> bool {
    value_kind(effect) == Some("createTokens") && integer_value(&effect["quantity"]).is_none()
}

impl GameEngine {
    fn token_creator(
        &self,
        effect: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<String, EngineError> {
        let creator = match effect.get("controller") {
            None => stack.controller.clone(),
            Some(reference) => self
                .resolve_player(reference, stack, bindings)
                .ok_or_else(|| EngineError::new("explicit token creator is unresolved"))?,
        };
        self.player_index(&creator)?;
        Ok(creator)
    }

    pub(super) fn token_creation_quantity<P: DecisionProvider>(
        &self,
        amount: i32,
        controller: &str,
        definition: &CardDefinition,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        let mut template;
        let definition = if definition.is_token {
            definition
        } else {
            template = definition.clone();
            template.is_token = true;
            &template
        };
        self.resolve_quantity_event(
            QuantityEvent {
                amount,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Token {
                    controller,
                    definition,
                },
            },
            provider,
        )
        .map(|q| q.amount)
    }

    pub(super) fn create_tokens_with_provider<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<Vec<String>, EngineError> {
        self.with_quantity_program_transaction(
            effect,
            Some(&stack_object.card.definition),
            |engine| {
                engine.create_tokens_inner(effect, stack_object, bindings, decisions, provider)
            },
        )
    }

    pub(super) fn create_tokens_inner<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<Vec<String>, EngineError> {
        let requested_quantity = self
            .checked_runtime_numeric_expression(
                &effect["quantity"],
                stack_object,
                bindings,
                decisions,
                "createTokens.quantity",
            )?
            .max(0);
        if requested_quantity == 0 {
            return Ok(Vec::new());
        }
        let token_controller = self.token_creator(effect, stack_object, bindings)?;
        let controller_index = self.player_index(&token_controller)?;
        let token = &effect["token"];
        let catalog_definition = (value_kind(token) == Some("namedToken"))
            .then(|| token["name"].as_str())
            .flatten()
            .and_then(|name| {
                source_linked_token_definition(&stack_object.card.definition, name)
                    .or_else(|| compile_named_token_definition(name).ok().flatten())
            });
        let mut created_ids = Vec::new();
        let token_index = 0;

        let subtypes = token["subtypes"]
            .as_array()
            .map(|subtypes| {
                subtypes
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let generated_name = (!subtypes.is_empty()).then(|| format!("{subtypes} Token"));
        let name = token["name"]
            .as_str()
            .or(generated_name.as_deref())
            .unwrap_or("Token");
        let types = token["types"]
            .as_array()
            .map(|types| {
                types
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|types| !types.is_empty())
            .map(|types| {
                let supertypes = token["supertypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                if supertypes.is_empty() {
                    types
                } else {
                    format!("{} {types}", supertypes.join(" "))
                }
            })
            .map(|types| {
                if subtypes.is_empty() {
                    types
                } else {
                    format!("{types} - {subtypes}")
                }
            })
            .or_else(|| {
                catalog_definition
                    .as_ref()
                    .map(|definition| definition.type_line.clone())
            })
            .unwrap_or_else(|| {
                (value_kind(token) == Some("namedToken"))
                    .then(|| format!("Token - {name}"))
                    .unwrap_or_else(|| "Creature - Token".to_string())
            });
        let mut token_rules = token["abilities"]
            .as_array()
            .cloned()
            .or_else(|| {
                catalog_definition
                    .as_ref()
                    .map(|definition| definition.printed_rules().to_vec())
            })
            .unwrap_or_default();
        let color_indicator = token["colors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(color_name_to_symbol)
            .collect::<Vec<_>>();
        if !color_indicator.is_empty() {
            token_rules.push(json!({
                "kind": "rulesMarker",
                "text": format!("Color indicator: {}.", color_indicator.join(", ")),
                "colorIndicator": color_indicator,
            }));
        }
        let instance_id = format!(
            "{}:token:{}:{}:{}",
            stack_object.controller, stack_object.id, name, token_index
        );
        let attacking = effect["attacking"].as_bool().unwrap_or(false);
        let tapped = effect["tapped"].as_bool().unwrap_or(false);
        let mut flags = BTreeMap::new();
        if attacking {
            flags.insert("attacking".to_string(), true);
        }
        let token_power = token.get("power").and_then(|power| {
            self.runtime_numeric_expression(power, stack_object, bindings, decisions)
        });
        let token_toughness = token.get("toughness").and_then(|toughness| {
            self.runtime_numeric_expression(toughness, stack_object, bindings, decisions)
        });
        let created = CardInstance {
            instance_id: instance_id.clone(),
            definition: CardDefinition {
                id: catalog_definition
                    .as_ref()
                    .map(|definition| definition.id.clone())
                    .unwrap_or_else(|| format!("token:{}", name.to_ascii_lowercase())),
                name: name.to_string(),
                type_line: types,
                is_commander: false,
                is_token: true,
                is_game_piece: true,
                is_sideboard: false,
                mana_cost: catalog_definition
                    .as_ref()
                    .map(|definition| definition.mana_cost.clone())
                    .unwrap_or_default(),
                power: token_power.map(|value| value.to_string()).or_else(|| {
                    catalog_definition
                        .as_ref()
                        .and_then(|definition| definition.power.clone())
                }),
                toughness: token_toughness.map(|value| value.to_string()).or_else(|| {
                    catalog_definition
                        .as_ref()
                        .and_then(|definition| definition.toughness.clone())
                }),
                rules: token_rules,
            },
            printed_definition: None,
            owner: token_controller.clone(),
            controller: token_controller.clone(),
            tapped,
            summoning_sick: true,
            damage_marked: 0,
            power_modifier: 0,
            toughness_modifier: 0,
            counters: BTreeMap::new(),
            flags,
            battle_protector: None,
            attached_to: None,
        };
        let quantity = self.token_creation_quantity(
            requested_quantity,
            &token_controller,
            &created.definition,
            provider,
        )?;
        let token_prefix = format!(
            "{}:token:{}:{}:",
            stack_object.controller, stack_object.id, name
        );
        let mut next_token_index = self
            .state
            .events
            .iter()
            .filter(|event| {
                event.kind == "tokenCreated"
                    && event
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with(&token_prefix))
            })
            .count();
        let mut prepared = Vec::new();
        for _ in 0..quantity {
            let mut created = created.clone();
            let instance_id = format!("{token_prefix}{next_token_index}");
            next_token_index += 1;
            created.instance_id = instance_id.clone();
            created_ids.push(instance_id.clone());
            for counter in effect["counters"].as_array().into_iter().flatten() {
                let Some(counter_name) = counter["counter"].as_str() else {
                    continue;
                };
                let count = self
                    .runtime_numeric_expression(
                        &counter["count"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0);
                *created
                    .counters
                    .entry(counter_name.to_string())
                    .or_default() += count;
            }
            self.apply_enter_replacements(&mut created, provider)?;
            prepared.push((controller_index, created));
        }
        let created_tokens = self.install_prepared_battlefield_entries(prepared);
        for created in created_tokens {
            let instance_id = created.instance_id.clone();
            if attacking
                && let Some(attacker_id) = decisions
                    .get("triggeringPermanentId")
                    .and_then(Value::as_str)
                    .or(Some(stack_object.card.instance_id.as_str()))
                && let Some(assignment) = self
                    .state
                    .combat
                    .attackers
                    .iter()
                    .find(|assignment| assignment.attacker_id == attacker_id)
                    .cloned()
            {
                self.state.combat.attackers.push(DeclaredAttacker {
                    attacker_id: instance_id.clone(),
                    defender: assignment.defender,
                    defending_player_id: assignment.defending_player_id,
                });
            }
            if effect["sacrificeAtNextEndStep"].as_bool().unwrap_or(false) {
                self.state.rule_modifiers.push(json!({
                    "kind": "sacrificeAtEndStep",
                    "cardInstanceId": instance_id,
                    "notBeforeTurn": self.state.turn_number,
                }));
            }
            self.record_event(
                "tokenCreated",
                Some(token_controller.clone()),
                Some(created.instance_id.clone()),
                json!({
                    "attacking": attacking,
                    "tapped": tapped,
                }),
            );
            self.enqueue_token_created_triggers(&token_controller, &created);
            self.publish_prepared_enter_battlefield_triggers(&created);
        }
        Ok(created_ids)
    }

    pub(super) fn create_token_copy_of_permanent_inner<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let resolved_source_id;
        let source_id = match value_kind(&effect["object"]) {
            Some("self" | "abilitySource") => &stack_object.card.instance_id,
            _ => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                resolved_source_id = instance_id;
                &resolved_source_id
            }
        };
        let Some((source_player, source_index)) = self.permanent_position(source_id) else {
            return Ok(());
        };
        let source = self.state.players[source_player].battlefield[source_index].clone();
        let token_controller = self.token_creator(effect, stack_object, bindings)?;
        let controller_index = self.player_index(&token_controller)?;
        let base_quantity = effect
            .get("quantity")
            .and_then(|quantity| {
                self.runtime_numeric_expression(
                    quantity,
                    stack_object,
                    bindings,
                    &stack_object.decisions,
                )
            })
            .unwrap_or(1)
            .max(0) as usize;
        let mut definition = source
            .printed_definition
            .clone()
            .unwrap_or_else(|| source.definition.clone());
        definition.id = format!("token-copy:{}", definition.id);
        definition.is_token = true;
        definition.is_game_piece = true;
        definition.is_sideboard = false;
        if effect["removeLegendary"].as_bool().unwrap_or(false) {
            definition.type_line = definition
                .type_line
                .split_whitespace()
                .filter(|part| !part.eq_ignore_ascii_case("legendary"))
                .collect::<Vec<_>>()
                .join(" ");
        }
        for keyword in effect["grantKeywords"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            definition.printed_rules_mut().push(json!({
                "kind": "keywordAbility",
                "source": { "kind": "self" },
                "ability": { "kind": keyword },
            }));
        }
        let quantity = self.token_creation_quantity(
            i32::try_from(base_quantity)
                .map_err(|_| EngineError::new("token quantity overflow"))?,
            &token_controller,
            &definition,
            provider,
        )?;
        let mut prepared = Vec::new();
        for copy_index in 0..quantity {
            let definition = definition.clone();
            self.copy_sequence += 1;
            let instance_id = format!(
                "{}:token-copy:{}:{}:{copy_index}",
                token_controller, stack_object.id, self.copy_sequence
            );
            let mut created = CardInstance {
                instance_id: instance_id.clone(),
                printed_definition: Some(definition.clone()),
                definition,
                owner: token_controller.clone(),
                controller: token_controller.clone(),
                tapped: false,
                summoning_sick: true,
                damage_marked: 0,
                power_modifier: 0,
                toughness_modifier: 0,
                counters: BTreeMap::new(),
                flags: BTreeMap::new(),
                battle_protector: None,
                attached_to: None,
            };
            self.apply_enter_replacements(&mut created, provider)?;
            prepared.push((controller_index, created));
        }
        let created_tokens = self.install_prepared_battlefield_entries(prepared);
        for created in created_tokens {
            let instance_id = created.instance_id.clone();
            if effect["exileAtNextEndStep"].as_bool().unwrap_or(false) {
                self.state.rule_modifiers.push(json!({
                    "kind": "exileAtEndStep",
                    "cardInstanceId": instance_id,
                    "notBeforeTurn": self.state.turn_number,
                }));
            }
            self.record_event(
                "tokenCreated",
                Some(token_controller.clone()),
                Some(instance_id.clone()),
                json!({ "copyOf": source.instance_id }),
            );
            self.enqueue_token_created_triggers(&token_controller, &created);
            self.publish_prepared_enter_battlefield_triggers(&created);
        }
        Ok(())
    }

    pub(super) fn create_modified_token_copy_inner<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let Some(TargetRef::Permanent { instance_id }) =
            self.resolve_target(&effect["object"], stack_object, bindings)
        else {
            return Ok(());
        };
        let Some((source_player, source_index)) = self.permanent_position(&instance_id) else {
            return Ok(());
        };
        let copied = self.state.players[source_player].battlefield[source_index].clone();
        let token_controller = self.token_creator(effect, stack_object, bindings)?;
        let controller_index = self.player_index(&token_controller)?;
        let mut definition = copied
            .printed_definition
            .clone()
            .unwrap_or_else(|| copied.definition.clone());
        definition.id = format!("token-copy:{}", definition.id);
        definition.is_token = true;
        definition.is_game_piece = true;
        definition.is_sideboard = false;
        if effect["removeLegendary"].as_bool().unwrap_or(false) {
            definition.type_line = definition
                .type_line
                .split_whitespace()
                .filter(|part| !part.eq_ignore_ascii_case("Legendary"))
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some(power) = integer_value(&effect["basePower"]) {
            definition.power = Some(power.to_string());
        }
        if let Some(toughness) = integer_value(&effect["baseToughness"]) {
            definition.toughness = Some(toughness.to_string());
        }
        for card_type in effect["addTypes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !type_line_contains(&definition.type_line, card_type) {
                definition.type_line = format!("{card_type} {}", definition.type_line);
            }
        }
        let added_subtypes = effect["addSubtypes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|subtype| !type_line_contains(&definition.type_line, subtype))
            .collect::<Vec<_>>();
        if !added_subtypes.is_empty() {
            if definition.type_line.contains(" - ") {
                definition.type_line =
                    format!("{} {}", definition.type_line, added_subtypes.join(" "));
            } else {
                definition.type_line =
                    format!("{} - {}", definition.type_line, added_subtypes.join(" "));
            }
        }
        let color_symbols = effect["addColors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(color_name_to_symbol)
            .collect::<Vec<_>>();
        if !color_symbols.is_empty() {
            definition.printed_rules_mut().push(json!({
                "kind": "rulesMarker",
                "text": format!("Color indicator: {}.", color_symbols.join(", ")),
                "colorIndicator": color_symbols,
            }));
        }
        for keyword in effect["grantKeywords"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            definition.printed_rules_mut().push(json!({
                "kind": "keywordAbility",
                "source": { "kind": "self" },
                "ability": { "kind": keyword },
            }));
        }
        if effect["diesDrawCard"].as_bool().unwrap_or(false) {
            definition.printed_rules_mut().push(json!({
                "kind": "triggeredAbility",
                "source": { "kind": "self" },
                "event": {
                    "kind": "permanentDied",
                    "object": { "kind": "self" },
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "count": { "kind": "integer", "value": 1 },
                }],
            }));
        }
        let quantity = self.token_creation_quantity(1, &token_controller, &definition, provider)?;
        let mut prepared = Vec::new();
        for copy_index in 0..quantity {
            let definition = definition.clone();
            self.copy_sequence += 1;
            let token_id = format!(
                "{}:modified-copy:{}:{}:{copy_index}",
                stack_object.controller, stack_object.id, self.copy_sequence
            );
            let mut token = CardInstance {
                instance_id: token_id.clone(),
                printed_definition: Some(definition.clone()),
                definition,
                owner: token_controller.clone(),
                controller: token_controller.clone(),
                tapped: false,
                summoning_sick: true,
                damage_marked: 0,
                power_modifier: 0,
                toughness_modifier: 0,
                counters: BTreeMap::new(),
                flags: BTreeMap::new(),
                battle_protector: None,
                attached_to: None,
            };
            if type_line_contains(&token.definition.type_line, "Aura") {
                let enchant_filter = token.definition.printed_rules().iter().find_map(|rule| {
                    (value_kind(rule) == Some("keywordAbility")
                        && value_kind(&rule["ability"]) == Some("enchant"))
                    .then(|| rule["ability"]["where"].clone())
                });
                let Some(enchant_filter) = enchant_filter else {
                    continue;
                };
                let candidates = self
                    .state
                    .players
                    .iter()
                    .filter(|player| !player.has_lost)
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        self.permanent_matches_effective_filter(permanent, &enchant_filter)
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                let selected = self.choose_resolution_objects(
                    stack_object,
                    provider,
                    &stack_object.controller,
                    "yennaAuraAttachment",
                    candidates,
                    1,
                    1,
                    "Choose a permanent for the Aura token to enchant.",
                )?;
                let Some(attached_to) = selected.into_iter().next() else {
                    continue;
                };
                token.attached_to = Some(attached_to);
            }
            for counter in effect["counters"].as_array().into_iter().flatten() {
                let Some(counter_name) = counter["counter"].as_str() else {
                    continue;
                };
                let count = self
                    .runtime_numeric_expression(
                        &counter["count"],
                        stack_object,
                        bindings,
                        &stack_object.decisions,
                    )
                    .unwrap_or(0)
                    .max(0);
                *token.counters.entry(counter_name.to_string()).or_default() += count;
            }
            self.apply_enter_replacements(&mut token, provider)?;
            prepared.push((controller_index, token));
        }
        let created_tokens = self.install_prepared_battlefield_entries(prepared);
        for token in created_tokens {
            let token_id = token.instance_id.clone();
            if effect["sacrificeAtNextEndStep"].as_bool().unwrap_or(false) {
                self.state.rule_modifiers.push(json!({
                    "kind": "sacrificeAtEndStep",
                    "cardInstanceId": token_id,
                    "notBeforeTurn": self.state.turn_number,
                }));
            }
            self.record_event(
                "tokenCreated",
                Some(token_controller.clone()),
                Some(token_id),
                json!({
                    "copyOf": instance_id,
                    "modifiedCopy": true,
                }),
            );
            self.enqueue_token_created_triggers(&token_controller, &token);
            self.publish_prepared_enter_battlefield_triggers(&token);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn create_tokens(
        &mut self,
        effect: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Vec<String>, EngineError> {
        self.create_tokens_with_provider(
            effect,
            stack,
            bindings,
            decisions,
            &mut super::quantity_replacement::OptionalQuantityProvider(None),
        )
    }

    pub(super) fn create_token_copy_of_permanent<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.with_quantity_program_transaction(effect, Some(&stack.card.definition), |engine| {
            engine.create_token_copy_of_permanent_inner(effect, stack, bindings, provider)
        })
    }

    pub(super) fn create_modified_token_copy<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.with_quantity_program_transaction(effect, Some(&stack.card.definition), |engine| {
            engine.create_modified_token_copy_inner(effect, stack, bindings, provider)
        })
    }
}
