//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("returnLinkedExiledCardToHand") => true,
        Some("moveTriggeringCardFromGraveyard") => {
            matches!(
                effect["to"].as_str(),
                Some("hand" | "battlefield" | "libraryBottom")
            ) && effect
                .get("controller")
                .is_none_or(player_reference_supported)
                && effect.get("tapped").is_none_or(Value::is_boolean)
        }

        Some("returnAbilitySourceFromGraveyard") => {
            effect["tapped"].is_boolean()
                && effect.get("counter").is_none_or(Value::is_string)
                && effect["grantKeywords"].as_array().is_none_or(|keywords| {
                    keywords.iter().all(|keyword| {
                        keyword
                            .as_str()
                            .is_some_and(|kind| keyword_kind_supported(&json!({ "kind": kind })))
                    })
                })
                && effect
                    .get("exileAtNextEndStep")
                    .is_none_or(Value::is_boolean)
                && effect
                    .get("exileIfLeavesBattlefield")
                    .is_none_or(Value::is_boolean)
        }

        Some("moveAbilitySourceToHand") => true,
        // CR 701.24b: the libraries that received these cards.
        Some("shuffleLibrariesHolding") => object_expression_supported(&effect["cards"]),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_movement_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_movement_domain_known(
            effect,
            next_effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_movement_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("moveTriggeringCardFromGraveyard") => {
                let Some(instance_id) = stack_object
                    .decisions
                    .get("triggeringPermanentId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                else {
                    return Ok(());
                };
                if !matches!(effect["to"].as_str(), Some("hand" | "libraryBottom"))
                    && self
                        .battlefield_entry_admission_for_id(&instance_id, "graveyard")
                        .is_prohibited()
                {
                    return Ok(());
                }
                let moved = self
                    .prepare_card_movement_source(&instance_id, None)?
                    .filter(|source| {
                        matches!(
                            source.zone,
                            super::super::card_zone_transition::ObjectZone::Graveyard(_)
                        )
                    })
                    .map(|source| self.take_prepared_movement_card(source))
                    .transpose()?;
                let Some(mut card) = moved else {
                    return Ok(());
                };
                let owner_id = card.owner.clone();
                if effect["to"].as_str() == Some("hand") {
                    let owner_index = self.player_index(&owner_id)?;
                    super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                    let destination =
                        super::super::card_zone_transition::ObjectZone::Hand(card.owner.clone());
                    self.commit_movement_identity(
                        &mut card,
                        &destination,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    self.state.players[owner_index].hand.push(card.card);
                } else if effect["to"].as_str() == Some("libraryBottom") {
                    let owner_index = self.player_index(&owner_id)?;
                    super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                    let destination =
                        super::super::card_zone_transition::ObjectZone::Library(card.owner.clone());
                    self.commit_movement_identity(
                        &mut card,
                        &destination,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    self.state.players[owner_index].library.insert(0, card.card);
                } else {
                    let controller_id = effect
                        .get("controller")
                        .and_then(|controller| {
                            self.resolve_player(controller, stack_object, bindings)
                        })
                        .unwrap_or_else(|| owner_id.clone());
                    let controller_index = self.player_index(&controller_id)?;
                    super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                    card.flags.insert("enteredFrom:graveyard".to_string(), true);
                    card.controller = controller_id;
                    card.summoning_sick = true;
                    self.apply_enter_replacements(&mut card, provider)?;
                    if effect["tapped"].as_bool() == Some(true) {
                        card.tapped = true;
                    }
                    self.commit_movement_identity(
                        &mut card,
                        &super::super::card_zone_transition::ObjectZone::Battlefield,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    let trigger_source = card.clone();
                    self.state.players[controller_index]
                        .battlefield
                        .push(card.card);
                    self.enqueue_enter_battlefield_triggers(&trigger_source);
                }
                self.enqueue_cards_left_graveyard_triggers(
                    &owner_id,
                    std::slice::from_ref(&instance_id),
                );
            }
            Some("shuffleLibrariesHolding") => {
                let cards = self
                    .resolve_runtime_object_ids_for_effect(
                        &effect["cards"],
                        stack_object,
                        bindings,
                        decisions,
                    )?
                    .unwrap_or_default();
                for player_id in self.libraries_holding(&cards) {
                    let player_index = self.player_index(&player_id)?;
                    super::super::library_shuffle::shuffle_library_excluding(
                        &mut self.state.players[player_index].library,
                        &[],
                        &mut self.rng,
                    );
                    self.record_event(
                        "zoneShuffled",
                        Some(player_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "zone": "library", "stackId": stack_object.id }),
                    );
                }
            }
            Some("moveAbilitySourceToHand") => {
                let player_index = self.player_index(&stack_object.card.owner)?;
                if let Some(card_index) = self.state.players[player_index]
                    .graveyard
                    .iter()
                    .position(|card| card.instance_id == stack_object.card.instance_id)
                {
                    let mut card =
                        self.take_zone_card_for_movement(player_index, "graveyard", card_index)?;
                    super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                    let destination =
                        super::super::card_zone_transition::ObjectZone::Hand(card.owner.clone());
                    self.commit_movement_identity(
                        &mut card,
                        &destination,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    self.state.players[player_index].hand.push(card.card);
                    self.record_event(
                        "abilitySourceMovedToHand",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "from": "graveyard" }),
                    );
                }
            }
            Some("returnLinkedExiledCardToHand") => {
                let Some(instance_id) = stack_object.decisions["linkedExiledCardId"].as_str()
                else {
                    return Ok(());
                };
                for player_index in 0..self.state.players.len() {
                    let Some(card_index) = self.state.players[player_index]
                        .exile
                        .iter()
                        .position(|card| card.instance_id == instance_id)
                    else {
                        continue;
                    };
                    let mut card =
                        self.take_zone_card_for_movement(player_index, "exile", card_index)?;
                    let owner_index = self.player_index(&card.owner)?;
                    card.flags.insert("faceDown".to_string(), false);
                    card.controller = card.owner.clone();
                    let destination =
                        super::super::card_zone_transition::ObjectZone::Hand(card.owner.clone());
                    self.commit_movement_identity(
                        &mut card,
                        &destination,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    self.state.players[owner_index].hand.push(card.card);
                    break;
                }
            }

            Some("returnAbilitySourceFromGraveyard") => {
                let instance_id = &stack_object.card.instance_id;
                if self
                    .battlefield_entry_admission_for_id(instance_id, "graveyard")
                    .is_prohibited()
                {
                    return Ok(());
                }
                let found = self
                    .prepare_card_movement_source(instance_id, None)?
                    .filter(|source| {
                        matches!(
                            source.zone,
                            super::super::card_zone_transition::ObjectZone::Graveyard(_)
                        )
                    })
                    .map(|source| self.take_prepared_movement_card(source))
                    .transpose()?;
                if let Some(mut card) = found {
                    let owner_id = card.owner.clone();
                    let owner_index = self.player_index(&owner_id)?;
                    super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                    card.flags.insert("enteredFrom:graveyard".to_string(), true);
                    card.controller = card.owner.clone();
                    card.tapped = effect["tapped"].as_bool().unwrap_or(false);
                    card.summoning_sick = true;
                    if let Some(counter) = effect["counter"].as_str() {
                        *card.counters.entry(counter.to_string()).or_default() += 1;
                    }
                    for keyword in effect["grantKeywords"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        card.definition.printed_rules_mut().push(json!({
                            "kind": "keywordAbility",
                            "source": { "kind": "self" },
                            "ability": { "kind": keyword },
                        }));
                    }
                    self.apply_enter_replacements(&mut card, provider)?;
                    if effect["tapped"].as_bool().unwrap_or(false) {
                        card.tapped = true;
                    }
                    self.commit_movement_identity(
                        &mut card,
                        &super::super::card_zone_transition::ObjectZone::Battlefield,
                        super::super::card_zone_transition::ObjectTransitionReason::Move,
                    )?;
                    let trigger_source = card.clone();
                    self.state.players[owner_index].battlefield.push(card.card);
                    if effect["exileAtNextEndStep"].as_bool().unwrap_or(false) {
                        self.state.rule_modifiers.push(json!({
                            "kind": "exileAtEndStep",
                            "cardInstanceId": instance_id,
                            "notBeforeTurn": self.state.turn_number,
                        }));
                    }
                    if effect["exileIfLeavesBattlefield"]
                        .as_bool()
                        .unwrap_or(false)
                    {
                        self.state.rule_modifiers.push(json!({
                            "kind": "exileIfLeavesBattlefield",
                            "cardInstanceId": instance_id,
                        }));
                    }
                    self.record_event(
                        "abilitySourceReturned",
                        Some(trigger_source.controller.clone()),
                        Some(trigger_source.instance_id.clone()),
                        json!({
                            "from": "graveyard",
                            "tapped": effect["tapped"],
                            "counter": effect["counter"],
                        }),
                    );
                    self.enqueue_cards_left_graveyard_triggers(
                        &owner_id,
                        std::slice::from_ref(instance_id),
                    );
                    self.enqueue_enter_battlefield_triggers(&trigger_source);
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
