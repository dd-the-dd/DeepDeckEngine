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
        if value_kind(&effect["token"]) == Some("copyOf") {
            return self.create_token_copies_inner(
                effect,
                stack_object,
                bindings,
                decisions,
                provider,
                requested_quantity,
            );
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
        let flags = BTreeMap::new();
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
        // CR 111.3, 303.4f: an Aura token created attached to an object
        // enters attached to it; without a legal object it is not created.
        let attach_host = match effect.get("attachTo") {
            None => None,
            Some(object) => {
                let host = self.token_copy_source_id(object, stack_object, bindings, decisions)?;
                match host.filter(|host| {
                    self.permanent_position(host)
                        .is_some_and(|(player_index, card_index)| {
                            self.aura_could_enchant(
                                &created.definition,
                                &self.state.players[player_index].battlefield[card_index],
                            )
                        })
                }) {
                    Some(host) => Some(host),
                    None => return Ok(Vec::new()),
                }
            }
        };
        let mut prepared = Vec::new();
        for _ in 0..quantity {
            let mut created = created.clone();
            created.attached_to = attach_host.clone();
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
            if attacking {
                self.put_token_onto_battlefield_attacking(&instance_id, stack_object, provider)?;
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
                    "typeLine": created.definition.type_line,
                }),
            );
            self.enqueue_token_created_triggers(&token_controller, &created);
            self.publish_prepared_enter_battlefield_triggers(&created);
        }
        Ok(created_ids)
    }

    /// The one copied object of a `copyOf` token (CR 707.2): the source, a
    /// declared target, or a bound singular object. `None` when it no longer
    /// exists; several objects are an error, never a silent first pick.
    fn token_copy_source_id(
        &self,
        object: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Option<String>, EngineError> {
        let ids = match value_kind(object) {
            Some("self" | "abilitySource") => vec![stack_object.card.instance_id.clone()],
            Some("observedEventObject") => self
                .resolve_observed_event_permanent(object, stack_object)?
                .into_iter()
                .collect(),
            _ if target_reference_supported(object) => self
                .resolve_target(object, stack_object, bindings)
                .and_then(|target| match target {
                    TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                        Some(vec![instance_id])
                    }
                    TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
                })
                .unwrap_or_default(),
            _ => {
                self.resolve_runtime_object_ids_checked(object, stack_object, bindings, decisions)?
            }
        };
        match ids.as_slice() {
            [] => Ok(None),
            [id] => Ok(Some(id.clone())),
            _ => Err(EngineError::new("a token copy of more than one object")),
        }
    }

    /// The copiable values of an object (CR 707.2): its current values as
    /// modified by copy effects, its face-down status (CR 707.2, 708.2) or
    /// the face that is up (CR 707.8, 712.8), without the characteristic
    /// markers that resolved effects recorded on it (they are not copiable).
    /// A permanent is read on the battlefield; a card in another zone, or the
    /// source as it last existed, from its definition there.
    fn copiable_definition(&self, id: &str, stack_object: &StackObject) -> Option<CardDefinition> {
        let card = self
            .permanent_position(id)
            .map(|(player, index)| &self.state.players[player].battlefield[index])
            .or_else(|| self.card_outside_battlefield(id))
            .or_else(|| (stack_object.card.instance_id == id).then_some(&stack_object.card))?;
        Some(Self::token_copy_definition(card))
    }

    /// The definition a token copy of `card` is created with: the card's
    /// copiable values (CR 707.2) as a token (CR 111.8).
    fn token_copy_definition(card: &CardInstance) -> CardDefinition {
        let mut definition = Self::copiable_values(card);
        definition.id = format!("token-copy:{}", definition.id);
        definition.is_token = true;
        definition.is_game_piece = true;
        definition.is_sideboard = false;
        definition
    }

    /// The copiable values of `card` (CR 707.2-707.3). A copy effect or a
    /// face-down status replaces the object's definition with one of another
    /// identity: those values are its copiable values, so a copy of a Clone
    /// copies what the Clone copied (CR 707.3). Otherwise the printed card is
    /// read, restricted to the face that is up (CR 707.8, 712.8); a copy of
    /// one face is not a transforming object, so it carries no face list.
    pub(in crate::engine) fn copiable_values(card: &CardInstance) -> CardDefinition {
        let mut definition = match card.printed_definition.as_ref() {
            Some(printed) if printed.id == card.definition.id => {
                let face_index = usize::from(card.flags.get("transformed") == Some(&true));
                let mut face = card.clone();
                face.definition = printed.clone();
                if apply_card_face(&mut face, face_index) {
                    for rule in face.definition.printed_rules_mut() {
                        if value_kind(rule) == Some("rulesMarker")
                            && let Some(marker) = rule.as_object_mut()
                        {
                            marker.remove("transformFaces");
                        }
                    }
                    face.definition
                } else {
                    printed.clone()
                }
            }
            _ => card.definition.clone(),
        };
        definition
            .printed_rules_mut()
            .retain(|rule| !Self::is_resolved_characteristic_marker(rule));
        definition
    }

    /// CR 707.9d: a copy exception that provides a specific value for a
    /// characteristic does not copy the characteristic-defining abilities of
    /// the copied object that would define it. `defines` selects the
    /// unconditional self-defining modifiers of one characteristic; a static
    /// ability left without modifiers is removed.
    fn remove_copied_characteristic_definitions(
        definition: &mut CardDefinition,
        defines: impl Fn(&Value) -> bool,
    ) {
        let defining = |modifier: &Value| {
            value_kind(&modifier["objects"]) == Some("self")
                && modifier.get("condition").is_none()
                && defines(modifier)
        };
        definition.printed_rules_mut().retain_mut(|rule| {
            if value_kind(rule) != Some("staticAbility") || rule.get("condition").is_some() {
                return true;
            }
            let Some(modifiers) = rule["modifiers"].as_array_mut() else {
                return true;
            };
            let before = modifiers.len();
            modifiers.retain(|modifier| !defining(modifier));
            before == modifiers.len() || !modifiers.is_empty()
        });
    }

    /// A copy whose power and toughness the copy effect provides does not
    /// copy the layer-7a abilities that define them (CR 707.9d).
    pub(in crate::engine) fn remove_copied_power_toughness_definitions(
        definition: &mut CardDefinition,
    ) {
        Self::remove_copied_characteristic_definitions(definition, |modifier| {
            Self::is_layer_7a_modifier(modifier)
                && matches!(
                    value_kind(modifier),
                    Some("setBasePowerToughness" | "setPower" | "setToughness")
                )
        });
    }

    /// Applies one copiable-value exception of a token copy (CR 707.9b). The
    /// operations are the characteristic-change vocabulary without object or
    /// duration; each changes the copy's copiable values.
    pub(super) fn apply_copy_exception(
        &self,
        definition: &mut CardDefinition,
        exception: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        match value_kind(exception) {
            Some("setBasePowerToughness") => {
                let power = self.checked_runtime_numeric_expression(
                    &exception["power"],
                    stack_object,
                    bindings,
                    decisions,
                    "copyOf.except.power",
                )?;
                let toughness = self.checked_runtime_numeric_expression(
                    &exception["toughness"],
                    stack_object,
                    bindings,
                    decisions,
                    "copyOf.except.toughness",
                )?;
                Self::remove_copied_power_toughness_definitions(definition);
                definition.power = Some(power.to_string());
                definition.toughness = Some(toughness.to_string());
            }
            // CR 707.9b: the copy has no mana cost (embalm, eternalize).
            Some("removeManaCost") => definition.mana_cost = String::new(),
            // CR 707.9b: `except its name is NAME`.
            Some("setName") => {
                definition.name = exception["name"].as_str().unwrap_or_default().to_string();
            }
            // CR 205.4, 707.9b: `except it's legendary`.
            Some("addSupertype") => {
                let supertype = exception["supertype"].as_str().unwrap_or_default();
                if !definition
                    .type_line
                    .split_whitespace()
                    .any(|part| part.eq_ignore_ascii_case(supertype))
                {
                    definition.type_line = format!("{supertype} {}", definition.type_line);
                }
            }
            Some("removeSupertype") => {
                let supertype = exception["supertype"].as_str().unwrap_or_default();
                definition.type_line = definition
                    .type_line
                    .split(' ')
                    .filter(|part| !part.eq_ignore_ascii_case(supertype))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            Some("addCardType") => {
                let card_type = exception["cardType"].as_str().unwrap_or_default();
                if !type_line_contains(&definition.type_line, card_type) {
                    definition.type_line = match definition.type_line.split_once(" - ") {
                        Some((types, subtypes)) => format!("{types} {card_type} - {subtypes}"),
                        None => format!("{} {card_type}", definition.type_line),
                    };
                }
            }
            Some("addSubtypes") => {
                let added = exception["subtypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|subtype| !type_line_contains(&definition.type_line, subtype))
                    .collect::<Vec<_>>();
                if !added.is_empty() {
                    definition.type_line = if definition.type_line.contains(" - ") {
                        format!("{} {}", definition.type_line, added.join(" "))
                    } else {
                        format!("{} - {}", definition.type_line, added.join(" "))
                    };
                }
            }
            Some("setColors") => {
                // The copy's colors are a copiable value: the same color-set
                // characteristic the color queries read (CR 105.3, 707.9b).
                // The copied color-defining abilities, devoid among them
                // (CR 702.114a), are not copied (CR 707.9d).
                Self::remove_copied_characteristic_definitions(definition, |modifier| {
                    matches!(value_kind(modifier), Some("setAllColors" | "setColor"))
                });
                definition.printed_rules_mut().retain(|rule| {
                    !(value_kind(rule) == Some("keywordAbility")
                        && value_kind(&rule["ability"]) == Some("devoid"))
                });
                definition
                    .printed_rules_mut()
                    .push(Self::copiable_color_set(&exception["colors"]));
            }
            Some("grantKeyword") => {
                definition.printed_rules_mut().push(json!({
                    "kind": "keywordAbility",
                    "source": { "kind": "self" },
                    "ability": { "kind": exception["keyword"] },
                }));
            }
            Some("grantAbility") => {
                definition
                    .printed_rules_mut()
                    .push(exception["ability"].clone());
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported token copy exception: {}",
                    value_kind(exception).unwrap_or("missingKind")
                )));
            }
        }
        Ok(())
    }

    /// `createTokens` whose token is `copyOf` an object (CR 111.8, 707.2):
    /// the same quantity replacement, entry, counters and created-token
    /// binding as a described token. A copy that is an Aura is attached as
    /// it enters (CR 303.4f).
    pub(super) fn create_token_copies_inner<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
        requested_quantity: i32,
    ) -> Result<Vec<String>, EngineError> {
        let token = &effect["token"];
        let copied =
            match self.token_copy_source_id(&token["object"], stack_object, bindings, decisions)? {
                Some(copied_id) => self
                    .copiable_definition(&copied_id, stack_object)
                    .map(|definition| (copied_id, definition)),
                // The event's object that left the battlefield is copied
                // with its copiable values as it last existed (CR 608.2h,
                // 707.2), from its authenticated departure capture.
                None => self
                    .departed_event_object_capture(&token["object"], stack_object)
                    .map(|card| (card.instance_id.clone(), Self::token_copy_definition(card))),
            };
        let Some((copied_id, mut definition)) = copied else {
            return Ok(Vec::new());
        };
        for exception in token["except"].as_array().into_iter().flatten() {
            self.apply_copy_exception(
                &mut definition,
                exception,
                stack_object,
                bindings,
                decisions,
            )?;
        }
        let token_controller = self.token_creator(effect, stack_object, bindings)?;
        let controller_index = self.player_index(&token_controller)?;
        let quantity = self.token_creation_quantity(
            requested_quantity,
            &token_controller,
            &definition,
            provider,
        )?;
        let tapped = effect["tapped"].as_bool().unwrap_or(false);
        let attacking = effect["attacking"].as_bool().unwrap_or(false);
        let mut created_ids = Vec::new();
        let mut prepared = Vec::new();
        for copy_index in 0..quantity {
            self.copy_sequence += 1;
            let instance_id = format!(
                "{}:token-copy:{}:{}:{copy_index}",
                token_controller, stack_object.id, self.copy_sequence
            );
            let mut created = CardInstance {
                instance_id: instance_id.clone(),
                printed_definition: Some(definition.clone()),
                definition: definition.clone(),
                owner: token_controller.clone(),
                controller: token_controller.clone(),
                tapped,
                summoning_sick: true,
                damage_marked: 0,
                power_modifier: 0,
                toughness_modifier: 0,
                counters: BTreeMap::new(),
                flags: BTreeMap::new(),
                battle_protector: None,
                attached_to: None,
            };
            match self.choose_entering_aura_host(
                &created.definition,
                &created.controller.clone(),
                stack_object,
                provider,
                "auraTokenAttachment",
            )? {
                super::aura_entry::EnteringAuraHost::NotAnAura => {}
                super::aura_entry::EnteringAuraHost::Host(host) => {
                    created.attached_to = Some(host);
                }
                // CR 303.4f: an Aura with nothing it could enchant stays
                // where it is; a token is simply not created.
                super::aura_entry::EnteringAuraHost::NoLegalHost => continue,
            }
            for counter in effect["counters"].as_array().into_iter().flatten() {
                let Some(counter_name) = counter["counter"].as_str() else {
                    continue;
                };
                let count = self
                    .checked_runtime_numeric_expression(
                        &counter["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "createTokens.counters",
                    )?
                    .max(0);
                *created
                    .counters
                    .entry(counter_name.to_string())
                    .or_default() += count;
            }
            self.apply_enter_replacements(&mut created, provider)?;
            created_ids.push(instance_id);
            prepared.push((controller_index, created));
        }
        let created_tokens = self.install_prepared_battlefield_entries(prepared);
        for created in created_tokens {
            let instance_id = created.instance_id.clone();
            if attacking {
                self.put_token_onto_battlefield_attacking(&instance_id, stack_object, provider)?;
            }
            for (flag, kind) in [
                ("sacrificeAtNextEndStep", "sacrificeAtEndStep"),
                ("exileAtNextEndStep", "exileAtEndStep"),
            ] {
                if effect[flag].as_bool().unwrap_or(false) {
                    self.state.rule_modifiers.push(json!({
                        "kind": kind,
                        "cardInstanceId": instance_id,
                        "notBeforeTurn": self.state.turn_number,
                    }));
                }
            }
            self.record_event(
                "tokenCreated",
                Some(token_controller.clone()),
                Some(instance_id.clone()),
                json!({
                    "copyOf": copied_id,
                    "attacking": attacking,
                    "tapped": tapped,
                    "typeLine": created.definition.type_line,
                }),
            );
            self.enqueue_token_created_triggers(&token_controller, &created);
            self.ensure_prepared_spell_copy(&instance_id)?;
            self.publish_prepared_enter_battlefield_triggers(&created);
        }
        Ok(created_ids)
    }

    /// CR 704.5y, 303.7a: when a permanent has more than one Role attached
    /// to it controlled by the same player, each of them except the one with
    /// the latest timestamp is put into its owner's graveyard. A Role enters
    /// attached (CR 111.3) and a controller's permanents are kept in entry
    /// order, so the latest one of a controller is the last attached.
    pub(super) fn put_superseded_roles_into_graveyard(&mut self) -> Result<(), EngineError> {
        let mut latest = BTreeMap::<(String, String), String>::new();
        let mut superseded = Vec::new();
        for permanent in self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
        {
            let Some(host) = permanent.attached_to.as_ref() else {
                continue;
            };
            if !type_line_contains(&permanent.definition.type_line, "Role") {
                continue;
            }
            if let Some(previous) = latest.insert(
                (permanent.controller.clone(), host.clone()),
                permanent.instance_id.clone(),
            ) {
                superseded.push(previous);
            }
        }
        for instance_id in superseded {
            self.move_permanent_to_graveyard(&instance_id, "roleUniqueness")?;
        }
        Ok(())
    }

    /// CR 508.4: a token put onto the battlefield attacking was never
    /// declared as an attacker; its controller chooses the player,
    /// planeswalker or battle it attacks among those the active player could
    /// attack. Outside the combat steps, or under the control of a player
    /// other than the active player, it enters but is not attacking
    /// (CR 506.3, 508.4a).
    fn put_token_onto_battlefield_attacking<P: DecisionProvider>(
        &mut self,
        token_id: &str,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.put_permanent_into_combat_attacking(
            token_id,
            &super::entering_attacking::AdmittedDefenders::Any,
            stack_object,
            provider,
        )
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
}
