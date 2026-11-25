//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

#[derive(Clone)]
struct CardMovementOrigin {
    zone: &'static str,
    player_id: String,
}

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    value_kind(value) == Some("moveCards")
}

fn destination_uses_each_owner(destination: &Value) -> bool {
    battlefield_entry_batch::destination_uses_each_owner(destination)
        || (matches!(
            value_kind(destination),
            Some("exile" | "hand" | "library" | "graveyard")
        ) && value_kind(&destination["player"]) == Some("ownerOf")
            && value_kind(&destination["player"]["object"]) == Some("candidate"))
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("moveCards") => {
            object_expression_supported(&effect["cards"])
                && effect.get("bind").is_none_or(Value::is_string)
                && effect.get("from").is_none_or(|from| {
                    matches!(
                        value_kind(from),
                        Some("library" | "hand" | "graveyard" | "exile")
                    ) && player_reference_supported(&from["player"])
                })
                && match value_kind(&effect["to"]) {
                    // A hand, library or graveyard is always the card's
                    // owner's (CR 400.3): each card selects its own.
                    Some("hand") => {
                        player_reference_supported(&effect["to"]["player"])
                            || destination_uses_each_owner(&effect["to"])
                    }
                    Some("ownersHand") => true,
                    Some("graveyard") => {
                        player_reference_supported(&effect["to"]["player"])
                            || destination_uses_each_owner(&effect["to"])
                    }
                    Some("battlefield") => {
                        (player_reference_supported(&effect["to"]["player"])
                            || battlefield_entry_batch::destination_uses_each_owner(&effect["to"]))
                            && effect["to"]["tapped"].is_boolean()
                            && effect["to"].get("faceChange").is_none_or(|face_change| {
                                value_kind(face_change) == Some("transformFace")
                            })
                            // CR 701.40a: manifest puts the card onto the
                            // battlefield face down as a 2/2 creature.
                            && effect["to"].get("faceDown").is_none_or(Value::is_boolean)
                            && !(effect["to"]["faceDown"].as_bool() == Some(true)
                                && effect["to"].get("faceChange").is_some())
                            && effect["to"]
                                .get("enterWithCounters")
                                .is_none_or(|counters| {
                                    counters.as_array().is_some_and(|counters| {
                                        counters.iter().all(|counter| {
                                            counter["counter"].is_string()
                                                && numeric_expression_supported(&counter["count"])
                                        })
                                    })
                                })
                            && ["addTypes", "addSubtypes"].iter().all(|key| {
                                effect["to"].get(*key).is_none_or(|values| {
                                    values
                                        .as_array()
                                        .is_some_and(|values| values.iter().all(Value::is_string))
                                })
                            })
                            && effect["to"]
                                .get("basePower")
                                .is_none_or(|value| integer_value(value).is_some())
                            && effect["to"]
                                .get("baseToughness")
                                .is_none_or(|value| integer_value(value).is_some())
                            && effect["to"]
                                .get("retainExistingTypes")
                                .is_none_or(Value::is_boolean)
                            // CR 508.4: `tapped and attacking`.
                            && effect["to"].get("attacking").is_none_or(|policy| {
                                effect["to"]["tapped"].as_bool() == Some(true)
                                    && super::super::entering_attacking::entering_attack_supported(
                                        policy,
                                    )
                            })
                    }
                    Some("library") => {
                        (player_reference_supported(&effect["to"]["player"])
                            || destination_uses_each_owner(&effect["to"]))
                            && matches!(effect["to"]["position"].as_str(), Some("top" | "bottom"))
                            // CR 401.2: `ORDINAL from the top` keeps N cards
                            // above the placed card.
                            && effect["to"].get("cardsAbove").is_none_or(|above| {
                                effect["to"]["position"].as_str() == Some("top")
                                    && numeric_expression_supported(above)
                            })
                    }
                    Some("exile") => {
                        (player_reference_supported(&effect["to"]["player"])
                            || destination_uses_each_owner(&effect["to"]))
                            && effect["to"]["faceDown"].is_boolean()
                            // `exile it with N KIND counters on it` (CR
                            // 702.62a): the counters are placed as it is
                            // exiled, face up only (a face-down card keeps
                            // no visible counters this frame can name).
                            && effect["to"].get("enterWithCounters").is_none_or(|counters| {
                                effect["to"]["faceDown"].as_bool() == Some(false)
                                    && exile_counters_supported(counters)
                            })
                    }
                    _ => false,
                }
        }
        Some("moveCard") => {
            value_kind(&effect["card"]) == Some("topCard")
                && library_zone_supported(&effect["card"]["zone"])
                && value_kind(&effect["to"]) == Some("exile")
                && effect["bind"].is_string()
        }
        Some("moveTargetCard") => {
            target_reference_supported(&effect["card"])
                && matches!(
                    effect["to"].as_str(),
                    Some("hand" | "battlefield" | "exile" | "libraryBottom" | "libraryTop")
                )
                && effect["tapped"].is_boolean()
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
        }
        Some("move") => move_operation_supported(effect),

        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_card_movement_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_card_movement_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_card_movement_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("moveCards") => {
                for reference in [effect.get("from"), effect.get("to")].into_iter().flatten() {
                    if self.value_requires_initially_illegal_target_in_context(
                        reference,
                        stack_object,
                        bindings,
                        decisions,
                    )? {
                        return Ok(());
                    }
                }
                let Some(objects) = self.resolve_runtime_object_ids_for_effect(
                    &effect["cards"],
                    stack_object,
                    bindings,
                    decisions,
                )?
                else {
                    return Ok(());
                };
                let objects = objects
                    .into_iter()
                    .filter(|id| self.nonbattlefield_card_can_change_zone(id))
                    .collect::<Vec<_>>();
                // The exile counters are counted once, before the move.
                let resolved_to = self.resolved_exile_counters_destination(
                    &effect["to"],
                    stack_object,
                    bindings,
                    decisions,
                );
                let objects =
                    self.claim_resolving_spell_movement(&resolved_to, stack_object, objects);
                let source_constraint = if let Some(from) = effect.get("from") {
                    let zone_name = value_kind(from)
                        .ok_or_else(|| EngineError::new("missing movement source zone"))?;
                    let player_id = self
                        .resolve_player(&from["player"], stack_object, bindings)
                        .ok_or_else(|| EngineError::new("unresolved movement source player"))?;
                    Some((zone_name.to_string(), self.player_index(&player_id)?))
                } else {
                    None
                };
                if value_kind(&effect["to"]) == Some("ownersHand") {
                    let mut moved = Vec::new();
                    for object in &objects {
                        let card = self
                            .prepare_card_movement_source(object, source_constraint.as_ref())?
                            .map(|source| self.take_prepared_movement_card(source))
                            .transpose()?;
                        if let Some(card) = card {
                            moved.push(card);
                        }
                    }
                    let receipts = self.put_owned_cards_in_owners_private_zones(
                        moved,
                        super::super::card_movement::PrivateZoneDestination::Hand,
                    )?;
                    let moved_ids = receipts
                        .iter()
                        .map(|receipt| receipt.card_id.clone())
                        .collect::<Vec<_>>();
                    if let Some(binding) = effect["bind"].as_str() {
                        bindings.insert(
                            binding.to_string(),
                            RuntimeBinding::Objects(moved_ids.clone()),
                        );
                    }
                    if moved_ids.is_empty() {
                        return Ok(());
                    }
                    self.state.rule_modifiers.retain(|modifier| {
                        value_kind(modifier) != Some("exiledWithSource")
                            || modifier["cardInstanceId"]
                                .as_str()
                                .is_none_or(|id| !moved_ids.iter().any(|moved| moved == id))
                    });
                    let owners = receipts
                        .into_iter()
                        .map(|receipt| (receipt.card_id, receipt.owner_id))
                        .collect::<BTreeMap<_, _>>();
                    self.record_event("cardsMoved", Some(stack_object.controller.clone()), Some(stack_object.card.instance_id.clone()), json!({"cardIds":moved_ids,"destination":effect["to"],"zoneOwnersByCard":owners}));
                    return Ok(());
                }
                let destination_uses_each_owner = destination_uses_each_owner(&effect["to"]);
                let player_id = if destination_uses_each_owner {
                    // Only a frame default: each moved card selects its owner.
                    stack_object.controller.clone()
                } else {
                    self.resolve_player(&effect["to"]["player"], stack_object, bindings)
                        .ok_or_else(|| EngineError::new("unresolved movement destination player"))?
                };
                let player_index = self.player_index(&player_id)?;
                let destination = value_kind(&effect["to"]).unwrap_or_default().to_string();
                if !matches!(
                    destination.as_str(),
                    "hand" | "graveyard" | "library" | "exile" | "battlefield"
                ) {
                    return Err(EngineError::new("unsupported card movement destination"));
                }
                let mut graveyard_destinations = BTreeMap::new();
                if destination == "graveyard" {
                    for object in &objects {
                        if self
                            .prepare_card_movement_source(object, source_constraint.as_ref())?
                            .is_some()
                        {
                            graveyard_destinations.insert(
                                object.clone(),
                                self.prepare_card_graveyard_destination(object)?,
                            );
                        }
                    }
                }
                if destination == "library" {
                    let hand_player_ids = self
                        .state
                        .players
                        .iter()
                        .filter(|player| {
                            player.hand.iter().any(|card| {
                                objects
                                    .iter()
                                    .any(|instance_id| instance_id == &card.instance_id)
                            })
                        })
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>();
                    for hand_player_id in hand_player_ids {
                        self.forget_known_hand_cards(&hand_player_id);
                    }
                    self.forget_known_hand_card_ids(&objects);
                }
                // One movement uses the policy active before any source removal.
                let entry_policy =
                    (destination == "battlefield").then(|| self.capture_zone_entry_policy());
                let mut controllers_by_card = BTreeMap::<String, String>::new();
                let mut aura_hosts = BTreeMap::<String, String>::new();
                let mut exile_origins = BTreeMap::<String, CardMovementOrigin>::new();
                let mut moved = Vec::new();
                let mut moved_ids = Vec::new();
                for object in &objects {
                    if source_constraint.is_none()
                        && destination == "exile"
                        && self.permanent_position(object).is_some()
                    {
                        if destination_uses_each_owner {
                            let (owner_player, owner_card) =
                                self.permanent_position(object).ok_or_else(|| {
                                    EngineError::new("moving permanent disappeared before exile")
                                })?;
                            controllers_by_card.insert(
                                object.clone(),
                                self.state.players[owner_player].battlefield[owner_card]
                                    .owner
                                    .clone(),
                            );
                        }
                        self.exile_permanent(object)?;
                        self.place_exile_counters(object, &resolved_to);
                        moved_ids.push(object.clone());
                        continue;
                    }
                    // A permanent named by a typed reference leaves the
                    // battlefield through its zone-specific lifecycle.
                    if source_constraint.is_none()
                        && self.move_referenced_permanent(object, &effect["to"])?
                    {
                        moved_ids.push(object.clone());
                        continue;
                    }
                    let source_zone = (destination == "battlefield")
                        .then(|| {
                            if let Some((zone_name, player_index)) = &source_constraint {
                                let player = &self.state.players[*player_index];
                                let cards = match zone_name.as_str() {
                                    "library" => &player.library,
                                    "hand" => &player.hand,
                                    "graveyard" => &player.graveyard,
                                    "exile" => &player.exile,
                                    _ => return None,
                                };
                                cards
                                    .iter()
                                    .any(|candidate| candidate.instance_id == *object)
                                    .then_some(zone_name.as_str())
                            } else {
                                self.state.players.iter().find_map(|player| {
                                    [
                                        ("library", &player.library),
                                        ("hand", &player.hand),
                                        ("graveyard", &player.graveyard),
                                        ("exile", &player.exile),
                                    ]
                                    .into_iter()
                                    .find_map(
                                        |(zone, cards)| {
                                            cards
                                                .iter()
                                                .any(|candidate| candidate.instance_id == *object)
                                                .then_some(zone)
                                        },
                                    )
                                })
                            }
                        })
                        .flatten();
                    if destination == "battlefield" {
                        let replacement = self
                            .card_outside_battlefield(object)
                            .zip(source_zone)
                            .and_then(|(card, zone)| {
                                let policy = entry_policy
                                    .as_ref()
                                    .expect("battlefield movement captured an entry policy");
                                if policy.admission(&card.definition, zone).is_prohibited() {
                                    return Some("prohibited");
                                }
                                let mut entering_card = card.clone();
                                if value_kind(&effect["to"]["faceChange"]) == Some("transformFace")
                                {
                                    apply_card_face(&mut entering_card, 1);
                                }
                                // CR 701.40f: a prohibition applies to the
                                // face-down object that would enter.
                                if effect["to"]["faceDown"].as_bool() == Some(true) {
                                    entering_card.definition =
                                        face_down_creature_definition(&entering_card.definition);
                                }
                                policy.destination_replacement(&entering_card.definition)
                            });
                        if let Some(replacement) = replacement {
                            if replacement == "exile"
                                && let Some(source) = self.prepare_card_movement_source(
                                    object,
                                    source_constraint.as_ref(),
                                )?
                            {
                                let mut card = self.take_prepared_movement_card(source)?;
                                let owner_index = self.player_index(&card.owner)?;
                                card.controller = card.owner.clone();
                                self.commit_movement_identity(&mut card,
                                    &super::super::card_zone_transition::ObjectZone::Exile,
                                    super::super::card_zone_transition::ObjectTransitionReason::Reexile)?;
                                self.state.players[owner_index].exile.push(card.card);
                            }
                            continue;
                        }
                        // CR 303.4f: an Aura chooses what it enchants as it
                        // enters; with nothing legal it stays where it is.
                        if effect["to"]["faceDown"].as_bool() != Some(true)
                            && let Some(card) = self.card_outside_battlefield(object)
                        {
                            let mut entering_card = card.clone();
                            if value_kind(&effect["to"]["faceChange"]) == Some("transformFace") {
                                apply_card_face(&mut entering_card, 1);
                            }
                            let chooser = if destination_uses_each_owner {
                                entering_card.owner.clone()
                            } else {
                                player_id.clone()
                            };
                            match self.choose_entering_aura_host(
                                &entering_card.definition,
                                &chooser,
                                stack_object,
                                provider,
                                "auraEntryAttachment",
                            )? {
                                super::super::aura_entry::EnteringAuraHost::NotAnAura => {}
                                super::super::aura_entry::EnteringAuraHost::Host(host) => {
                                    aura_hosts.insert(object.clone(), host);
                                }
                                super::super::aura_entry::EnteringAuraHost::NoLegalHost => {
                                    continue;
                                }
                            }
                        }
                    }
                    let card = self
                        .prepare_card_movement_source(object, source_constraint.as_ref())?
                        .map(|source| self.take_prepared_movement_card(source))
                        .transpose()?;
                    if let Some(mut card) = card {
                        if destination == "exile" {
                            exile_origins.insert(
                                card.instance_id.clone(),
                                CardMovementOrigin {
                                    zone: card.source.zone.name(),
                                    player_id: card.source.storage_player_id.clone(),
                                },
                            );
                        }
                        if let Some(source_zone) =
                            source_zone.filter(|_| destination == "battlefield")
                        {
                            card.flags
                                .insert(format!("enteredFrom:{source_zone}"), true);
                        }
                        moved_ids.push(card.instance_id.clone());
                        moved.push(card);
                    }
                }
                let mut private_receipts = Vec::new();
                match destination.as_str() {
                    "hand" | "graveyard" | "library" => {
                        use super::super::card_movement::PrivateZoneDestination;
                        if destination == "library"
                            && value_kind(&effect["order"]) == Some("random")
                        {
                            moved.shuffle(&mut self.rng);
                        }
                        let zone = match destination.as_str() {
                            "hand" => PrivateZoneDestination::Hand,
                            "graveyard" => PrivateZoneDestination::Graveyard,
                            "library" if effect["to"]["position"] == "bottom" => {
                                PrivateZoneDestination::LibraryBottom
                            }
                            _ => PrivateZoneDestination::LibraryTop,
                        };
                        if destination == "graveyard" {
                            for card in moved {
                                let id = card.instance_id.clone();
                                let owner = card.owner.clone();
                                let owner_index = self.player_index(&owner)?;
                                let plan = graveyard_destinations.remove(&id).ok_or_else(|| {
                                    EngineError::new(
                                        "movement graveyard destination was not prepared",
                                    )
                                })?;
                                let graveyard = self.put_owned_card_into_prepared_graveyard(
                                    owner_index,
                                    card,
                                    plan,
                                )?;
                                private_receipts.push(
                                    super::super::card_movement::PrivateZoneReceipt {
                                        card_id: id,
                                        owner_id: owner,
                                        actual_zone: if graveyard { "graveyard" } else { "exile" },
                                    },
                                );
                            }
                        } else {
                            private_receipts =
                                self.put_owned_cards_in_owners_private_zones(moved, zone)?;
                            // `ORDINAL from the top`: each placed card goes
                            // below that many cards of its owner's library.
                            if let Some(above) = effect["to"].get("cardsAbove") {
                                let above = self
                                    .runtime_numeric_expression(
                                        above,
                                        stack_object,
                                        bindings,
                                        decisions,
                                    )
                                    .unwrap_or(0)
                                    .max(0);
                                let above = usize::try_from(above).unwrap_or(0);
                                for receipt in &private_receipts {
                                    if receipt.actual_zone == "library" && above > 0 {
                                        let owner_index = self.player_index(&receipt.owner_id)?;
                                        self.place_library_card_beneath_top(
                                            owner_index,
                                            &receipt.card_id,
                                            above,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    "exile" => {
                        let face_down = effect["to"]["faceDown"].as_bool().unwrap_or(false);
                        for mut card in moved {
                            if face_down {
                                card.flags.insert("faceDown".to_string(), true);
                            } else {
                                card.flags.remove("faceDown");
                            }
                            let destination_index = if destination_uses_each_owner {
                                self.player_index(&card.owner)?
                            } else {
                                player_index
                            };
                            if destination_uses_each_owner {
                                controllers_by_card
                                    .insert(card.instance_id.clone(), card.owner.clone());
                                card.controller = card.owner.clone();
                            }
                            let exiled_id = card.instance_id.clone();
                            let destination_player_id =
                                self.state.players[destination_index].id.clone();
                            let origin = exile_origins.remove(&exiled_id).ok_or_else(|| {
                                EngineError::new(
                                    "moved exile card source provenance is unavailable",
                                )
                            })?;
                            self.commit_movement_identity(
                                &mut card,
                                &super::super::card_zone_transition::ObjectZone::Exile,
                                super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                            )?;
                            for (counter, count) in exile_counter_amounts(&resolved_to) {
                                *card.card.counters.entry(counter).or_default() += count;
                            }
                            self.state.players[destination_index].exile.push(card.card);
                            // Shared event path: owner selection changes only
                            // the destination, never whether a move is recorded.
                            self.record_event("cardExiled",Some(origin.player_id),Some(exiled_id),json!({"from":origin.zone,"reason":"effect","to":"exile","destinationPlayerId":destination_player_id}));
                        }
                    }
                    "battlefield" => {
                        let tapped = effect["to"]["tapped"].as_bool().unwrap_or(false);
                        let mut prepared_entries = Vec::with_capacity(moved.len());
                        for mut moved_card in moved {
                            // The card becomes a new object with no memory of
                            // its previous existence, and counters on it are
                            // not retained (CR 400.7, 122.2).
                            super::super::card_zone_transition::reset_owned_movement_card(
                                &mut moved_card,
                            )?;
                            let mut card = moved_card.card;
                            card.controller = if destination_uses_each_owner {
                                card.owner.clone()
                            } else {
                                player_id.clone()
                            };
                            card.tapped = tapped;
                            card.summoning_sick = true;
                            if let Some(host) = aura_hosts.remove(&card.instance_id) {
                                card.attached_to = Some(host);
                            }
                            if value_kind(&effect["to"]["faceChange"]) == Some("transformFace")
                                && !apply_card_face(&mut card, 1)
                            {
                                card.flags.insert("transformed".to_string(), true);
                            }
                            // CR 701.40a: the face-down 2/2 characteristics
                            // replace the printed ones until it is turned face
                            // up; the printed card is kept for that special action.
                            if effect["to"]["faceDown"].as_bool() == Some(true) {
                                if card.printed_definition.is_none() {
                                    card.printed_definition = Some(card.definition.clone());
                                }
                                card.definition = face_down_creature_definition(&card.definition);
                                card.flags.insert("faceDown".to_string(), true);
                            }
                            let added_types = effect["to"]["addTypes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|card_type| {
                                    !type_line_contains(&card.definition.type_line, card_type)
                                })
                                .collect::<Vec<_>>();
                            if !added_types.is_empty() {
                                card.definition.type_line = format!(
                                    "{} {}",
                                    card.definition.type_line,
                                    added_types.join(" ")
                                );
                            }
                            let added_subtypes = effect["to"]["addSubtypes"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|subtype| {
                                    !type_line_contains(&card.definition.type_line, subtype)
                                })
                                .collect::<Vec<_>>();
                            if !added_subtypes.is_empty() {
                                let separator = if card.definition.type_line.contains(" - ") {
                                    " "
                                } else {
                                    " - "
                                };
                                card.definition.type_line = format!(
                                    "{}{separator}{}",
                                    card.definition.type_line,
                                    added_subtypes.join(" ")
                                );
                            }
                            if let Some(power) = integer_value(&effect["to"]["basePower"]) {
                                card.definition.power = Some(power.to_string());
                            }
                            if let Some(toughness) = integer_value(&effect["to"]["baseToughness"]) {
                                card.definition.toughness = Some(toughness.to_string());
                            }
                            for counter in effect["to"]["enterWithCounters"]
                                .as_array()
                                .into_iter()
                                .flatten()
                            {
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
                                if count > 0 {
                                    *card.counters.entry(counter_name.to_string()).or_default() +=
                                        count;
                                }
                            }
                            self.apply_enter_replacements(&mut card, provider)?;
                            if tapped {
                                card.tapped = true;
                            }
                            let entry_player_index = self.player_index(&card.controller)?;
                            if destination_uses_each_owner {
                                controllers_by_card
                                    .insert(card.instance_id.clone(), card.controller.clone());
                            }
                            prepared_entries.push((entry_player_index, card));
                        }
                        self.commit_identity_checked_battlefield_entries(prepared_entries)?;
                        if let Some(policy) = effect["to"].get("attacking") {
                            let entered = moved_ids
                                .iter()
                                .filter(|id| self.permanent_position(id).is_some())
                                .cloned()
                                .collect::<Vec<_>>();
                            self.put_permanents_into_combat_attacking(
                                &entered,
                                policy,
                                stack_object,
                                bindings,
                                provider,
                            )?;
                        }
                    }
                    _ => {}
                }
                let actual_moved_ids = moved_ids.clone();
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(moved_ids));
                }
                if actual_moved_ids.is_empty() {
                    return Ok(());
                }
                let mut movement_detail = if destination_uses_each_owner && destination == "exile" {
                    json!({"cardIds":actual_moved_ids,"destination":effect["to"],"zoneOwnersByCard":controllers_by_card})
                } else if destination_uses_each_owner {
                    json!({ "cardIds": actual_moved_ids, "destination": effect["to"],
                        "controllersByCard": controllers_by_card })
                } else {
                    json!({ "cardIds": actual_moved_ids, "destination": effect["to"] })
                };
                if !private_receipts.is_empty() {
                    let owners = private_receipts
                        .iter()
                        .map(|receipt| (receipt.card_id.clone(), receipt.owner_id.clone()))
                        .collect::<BTreeMap<_, _>>();
                    let actual_zones = private_receipts
                        .iter()
                        .map(|receipt| (receipt.card_id.clone(), receipt.actual_zone))
                        .collect::<BTreeMap<_, _>>();
                    movement_detail["zoneOwnersByCard"] = json!(owners);
                    movement_detail["actualZonesByCard"] = json!(actual_zones);
                    movement_detail["requestedDestinationPlayerId"] = json!(player_id);
                }
                let source_id = if destination_uses_each_owner
                    && destination == "battlefield"
                    && stack_object.ability_kind.as_deref() == Some("immediateZoneTransition")
                {
                    None
                } else {
                    Some(stack_object.card.instance_id.clone())
                };
                self.record_event(
                    "cardsMoved",
                    (!destination_uses_each_owner).then_some(player_id),
                    source_id,
                    movement_detail,
                );
            }
            Some("moveCard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["card"]["zone"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let Some(card_index) = self.state.players[player_index]
                    .library
                    .len()
                    .checked_sub(1)
                else {
                    return Ok(());
                };
                let mut card =
                    self.take_zone_card_for_movement(player_index, "library", card_index)?;
                let card_id = card.instance_id.clone();
                self.commit_movement_identity(
                    &mut card,
                    &super::super::card_zone_transition::ObjectZone::Exile,
                    super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                )?;
                self.state.players[player_index].exile.push(card.card);
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(vec![card_id.clone()]),
                    );
                }
                self.record_event(
                    "cardMoved",
                    Some(player_id),
                    Some(card_id),
                    json!({ "from": "library", "position": "top", "to": "exile" }),
                );
            }
            Some("move") => {
                let Some(from_player_id) =
                    self.resolve_player(&effect["from"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let from_zone = value_kind(&effect["from"]).unwrap_or_default();
                let instance_ids = if value_kind(&effect["objects"]) == Some("chosenObjects") {
                    self.resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default()
                } else {
                    self.resolve_target(&effect["objects"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Card { instance_id } => Some(vec![instance_id]),
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                for instance_id in instance_ids {
                    self.move_card_to_exile(&instance_id, from_zone, &from_player_id, "effect")?;
                }
            }
            Some("moveTargetCard") => {
                if let Some(TargetRef::Card { instance_id }) =
                    self.resolve_target(&effect["card"], stack_object, bindings)
                {
                    let from_zone = self.state.players.iter().find_map(|player| {
                        [
                            ("library", &player.library),
                            ("hand", &player.hand),
                            ("graveyard", &player.graveyard),
                            ("exile", &player.exile),
                            ("sideboard", &player.sideboard),
                            ("commandZone", &player.command_zone),
                        ]
                        .into_iter()
                        .find_map(|(zone, cards)| {
                            cards
                                .iter()
                                .any(|card| card.instance_id == instance_id)
                                .then(|| (zone, player.id.clone()))
                        })
                    });
                    if effect["to"].as_str() == Some("battlefield") {
                        let replacement = from_zone.as_ref().and_then(|(zone, _)| {
                            self.card_outside_battlefield(&instance_id)
                                .and_then(|card| {
                                    self.card_entry_from_zone_replacement(&card.definition, zone)
                                })
                        });
                        if let Some(replacement) = replacement {
                            if replacement == "exile"
                                && let Some(source) =
                                    self.prepare_card_movement_source(&instance_id, None)?
                            {
                                let mut card = self.take_prepared_movement_card(source)?;
                                let owner_index = self.player_index(&card.owner)?;
                                card.controller = card.owner.clone();
                                self.commit_movement_identity(&mut card, &super::super::card_zone_transition::ObjectZone::Exile,
                                    super::super::card_zone_transition::ObjectTransitionReason::Reexile)?;
                                self.state.players[owner_index].exile.push(card.card);
                            }
                            return Ok(());
                        }
                    }
                    let found = self
                        .prepare_card_movement_source(&instance_id, None)?
                        .map(|source| self.take_prepared_movement_card(source))
                        .transpose()?;
                    if let Some(mut card) = found {
                        let owner_id = card.owner.clone();
                        let owner_index = self.player_index(&card.owner)?;
                        match effect["to"].as_str() {
                            Some("hand") => {
                                super::super::card_zone_transition::reset_owned_movement_card(
                                    &mut card,
                                )?;
                                card.controller = owner_id.clone();
                                let destination =
                                    super::super::card_zone_transition::ObjectZone::Hand(
                                        owner_id.clone(),
                                    );
                                self.commit_movement_identity(&mut card, &destination,
                                    super::super::card_zone_transition::ObjectTransitionReason::Move)?;
                                self.state.players[owner_index].hand.push(card.card);
                            }
                            Some("libraryBottom") => {
                                super::super::card_zone_transition::reset_owned_movement_card(
                                    &mut card,
                                )?;
                                card.controller = owner_id.clone();
                                let destination =
                                    super::super::card_zone_transition::ObjectZone::Library(
                                        owner_id.clone(),
                                    );
                                self.commit_movement_identity(&mut card, &destination,
                                    super::super::card_zone_transition::ObjectTransitionReason::Move)?;
                                self.state.players[owner_index].library.insert(0, card.card)
                            }
                            Some("libraryTop") => {
                                super::super::card_zone_transition::reset_owned_movement_card(
                                    &mut card,
                                )?;
                                card.controller = owner_id.clone();
                                let destination =
                                    super::super::card_zone_transition::ObjectZone::Library(
                                        owner_id.clone(),
                                    );
                                self.commit_movement_identity(&mut card, &destination,
                                    super::super::card_zone_transition::ObjectTransitionReason::Move)?;
                                self.state.players[owner_index].library.push(card.card)
                            }
                            Some("battlefield") => {
                                let controller_id = self
                                    .resolve_player(&effect["controller"], stack_object, bindings)
                                    .unwrap_or_else(|| owner_id.clone());
                                let controller_index = self.player_index(&controller_id)?;
                                super::super::card_zone_transition::reset_owned_movement_card(
                                    &mut card,
                                )?;
                                if let Some((zone, _)) = from_zone.as_ref() {
                                    card.flags.insert(format!("enteredFrom:{zone}"), true);
                                }
                                card.controller = controller_id.clone();
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
                                self.commit_movement_identity(&mut card, &super::super::card_zone_transition::ObjectZone::Battlefield, super::super::card_zone_transition::ObjectTransitionReason::Move)?;
                                let trigger_source = card.clone();
                                self.state.players[controller_index]
                                    .battlefield
                                    .push(card.card);
                                if effect["exileAtNextEndStep"].as_bool() == Some(true) {
                                    self.state.rule_modifiers.push(json!({
                                        "kind": "exileAtEndStep",
                                        "cardInstanceId": instance_id,
                                        "notBeforeTurn": self.state.turn_number,
                                    }));
                                }
                                self.record_event(
                                    "cardMovedToBattlefield",
                                    Some(controller_id),
                                    Some(instance_id.clone()),
                                    json!({
                                        "from": from_zone.as_ref().map(|(zone, _)| *zone),
                                        "tapped": effect["tapped"],
                                    }),
                                );
                                self.ensure_prepared_spell_copy(&instance_id)?;
                                self.enqueue_enter_battlefield_triggers(&trigger_source);
                            }
                            Some("exile") => {
                                super::super::card_zone_transition::reset_owned_movement_card(
                                    &mut card,
                                )?;
                                card.controller = owner_id.clone();
                                card.flags.insert("faceDown".to_string(), false);
                                self.commit_movement_identity(&mut card, &super::super::card_zone_transition::ObjectZone::Exile,
                                    super::super::card_zone_transition::ObjectTransitionReason::Reexile)?;
                                self.state.players[owner_index].exile.push(card.card);
                                self.record_event(
                                    "cardExiled",
                                    Some(owner_id.clone()),
                                    Some(instance_id.clone()),
                                    json!({
                                        "from": from_zone.as_ref().map(|(zone, _)| *zone),
                                        "reason": "effect",
                                    }),
                                );
                            }
                            _ => {}
                        }
                        if let Some(("graveyard", graveyard_owner_id)) = from_zone {
                            self.enqueue_cards_left_graveyard_triggers(
                                &graveyard_owner_id,
                                std::slice::from_ref(&instance_id),
                            );
                        }
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

/// `with N KIND counters on it` after an exile destination: whether every
/// placement names a counter kind and a supported amount.
fn exile_counters_supported(counters: &Value) -> bool {
    counters.as_array().is_some_and(|counters| {
        !counters.is_empty()
            && counters.iter().all(|counter| {
                counter["counter"].is_string() && numeric_expression_supported(&counter["count"])
            })
    })
}

/// The counter placements of a destination whose amounts were already
/// counted (`resolved_exile_counters_destination`).
fn exile_counter_amounts(destination: &Value) -> Vec<(String, i32)> {
    destination["enterWithCounters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|counter| {
            let name = counter["counter"].as_str()?;
            let count = integer_value(&counter["count"])?;
            (count > 0).then(|| (name.to_string(), count))
        })
        .collect()
}

impl GameEngine {
    /// The exile destination with each counter amount counted once, before
    /// any card moves (CR 608.2h); other destinations are returned unchanged.
    fn resolved_exile_counters_destination(
        &self,
        destination: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Value {
        let mut resolved = destination.clone();
        if value_kind(destination) != Some("exile") {
            return resolved;
        }
        if let Some(counters) = resolved
            .get_mut("enterWithCounters")
            .and_then(Value::as_array_mut)
        {
            for counter in counters {
                let count = self
                    .runtime_numeric_expression(
                        &counter["count"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0);
                counter["count"] = json!(count);
            }
        }
        resolved
    }

    /// Puts the destination's counters on a permanent that was just exiled
    /// (CR 702.62a): the exiled card is a new object (CR 400.7) that gets
    /// them as it is exiled.
    fn place_exile_counters(&mut self, instance_id: &str, destination: &Value) {
        let amounts = exile_counter_amounts(destination);
        if amounts.is_empty() {
            return;
        }
        if let Some(card) = self
            .state
            .players
            .iter_mut()
            .flat_map(|player| player.exile.iter_mut())
            .find(|card| card.instance_id == instance_id)
        {
            for (counter, count) in amounts {
                *card.counters.entry(counter).or_default() += count;
            }
        }
    }
}
