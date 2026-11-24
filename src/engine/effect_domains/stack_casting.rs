//! Stack actions and independently configured counter destinations.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("attachPermanent") => {
            (matches!(
                value_kind(&effect["attachment"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["attachment"])
                || object_expression_supported(&effect["attachment"]))
                && (target_reference_supported(&effect["to"])
                    || value_kind(&effect["to"]) == Some("decisionResult")
                        && effect["to"]["decisionId"].is_string()
                    || object_expression_supported(&effect["to"]))
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("castOneCard") => {
            player_reference_supported(&effect["player"])
                && object_expression_supported(&effect["cards"])
                && matches!(
                    effect["sourceZone"].as_str(),
                    Some("graveyard" | "hand" | "exile")
                )
                && effect["normalCostsApply"].is_boolean()
                && effect["optional"].is_boolean()
                && effect["bind"].is_string()
                && effect.get("spellWhere").is_none_or(stack_filter_supported)
        }
        Some("castAnyNumber") => {
            cast_any_number_supported(effect) && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("counterSpell") => {
            target_reference_supported(&effect["spell"])
                && effect.get("exileInstead").is_none_or(Value::is_boolean)
                && effect
                    .get("exileInsteadWhere")
                    .is_none_or(card_filter_supported)
                && effect.get("bindExiledAs").is_none_or(Value::is_string)
                && effect
                    .get("prohibitControllerSpellsThisTurn")
                    .is_none_or(Value::is_boolean)
                && effect.get("addManaAtNextMainPhase").is_none_or(|symbol| {
                    symbol
                        .as_str()
                        .is_some_and(|symbol| matches!(symbol, "W" | "U" | "B" | "R" | "G" | "C"))
                })
                && effect.get("controllerCreatesTokens").is_none_or(|tokens| {
                    tokens["name"].is_string() && numeric_expression_supported(&tokens["quantity"])
                })
        }
        Some("counterStackObject") => {
            target_reference_supported(&effect["object"])
                && effect.get("exileInstead").is_none_or(Value::is_boolean)
        }
        Some("counterOpponentStackObjects") => effect["bindCountAs"].is_string(),
        Some("suspendStackItem") => {
            target_reference_supported(&effect["object"])
                && integer_value(&effect["timeCounters"]).is_some_and(|count| count > 0)
        }
        Some("changeSingleTarget") => {
            target_reference_supported(&effect["object"])
                && player_reference_supported(&effect["player"])
        }
        Some("changeTargets") => {
            target_reference_supported(&effect["object"])
                && player_reference_supported(&effect["player"])
                && effect["optional"].is_boolean()
        }
        _ => false,
    }
}

fn cast_any_number_supported(effect: &Value) -> bool {
    let without_paying = effect["withoutPayingManaCost"].as_bool();
    let alternative_costs_allowed = effect["alternativeCostsAllowed"].as_bool();
    let payment_supported = match (
        without_paying,
        effect.get("alternativeCost").and_then(value_kind),
    ) {
        (Some(true), None) => alternative_costs_allowed == Some(false),
        (Some(false), None) => alternative_costs_allowed == Some(true),
        (Some(false), Some("payPlayerCounters")) => {
            alternative_costs_allowed == Some(false)
                && effect["alternativeCost"]["counter"].is_string()
                && value_kind(&effect["alternativeCost"]["count"]) == Some("manaValueOfCastCard")
        }
        // CR 118.9: life equal to the mana value of the spell actually cast.
        (Some(false), Some("payLife")) => {
            alternative_costs_allowed == Some(false)
                && value_kind(&effect["alternativeCost"]["amount"]) == Some("manaValueOfCastCard")
        }
        _ => false,
    };
    // The cards are an object expression, or the caster's own command zone
    // filtered by `cardsWhere` (CR 408.1: only its holder casts from it).
    let cards_supported = match (effect.get("cards"), effect.get("cardsWhere")) {
        (Some(cards), None) => object_expression_supported(cards),
        (None, Some(filter)) => {
            effect["sourceZone"].as_str() == Some("commandZone") && card_filter_supported(filter)
        }
        _ => false,
    };
    let zones_supported = match (effect.get("sourceZone"), effect.get("sourceZones")) {
        (None, None) => true,
        (Some(zone), None) => matches!(
            zone.as_str(),
            Some("exile" | "graveyard" | "hand" | "commandZone")
        ),
        (None, Some(zones)) => zones.as_array().is_some_and(|zones| {
            zones.len() >= 2
                && zones
                    .iter()
                    .all(|zone| matches!(zone.as_str(), Some("exile" | "graveyard" | "hand")))
        }),
        (Some(_), Some(_)) => false,
    };
    player_reference_supported(&effect["player"])
        && cards_supported
        && value_kind(&effect["where"]) == Some("canBeCastAsSpell")
        && value_kind(&effect["timing"]) == Some("duringResolution")
        && payment_supported
        && effect["additionalCostsApply"].as_bool() == Some(true)
        && integer_value(&effect["variableManaValue"]) == Some(0)
        && zones_supported
        && effect
            .get("maximum")
            .is_none_or(|maximum| integer_value(maximum).is_some_and(|value| value > 0))
        && effect
            .get("castVia")
            .is_none_or(suspend_keyword::cast_via_supported)
        && effect.get("mandatory").is_none_or(Value::is_boolean)
        // `you may play`: a land is played as the player's land play.
        && effect.get("playLands").is_none_or(|play| {
            play.is_boolean() && effect.get("mandatory").is_none_or(|m| m != &Value::Bool(true))
        })
        && effect
            .get("exileInsteadOfGraveyard")
            .is_none_or(cast_graveyard_exile_supported)
}

/// `If that spell would be put into a graveyard, exile it instead` (CR
/// 614.1a) on the spells one resolution-time cast casts: whose graveyard
/// (`controller`: only a spell the controller owns; `any`) and, for `a
/// PREDICATE spell cast this way`, the cards it applies to.
fn cast_graveyard_exile_supported(replacement: &Value) -> bool {
    matches!(
        replacement["graveyard"].as_str(),
        Some("controller" | "any")
    ) && replacement.get("where").is_none_or(card_filter_supported)
}

impl GameEngine {
    pub(super) fn execute_stack_casting_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_stack_casting_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_stack_casting_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("castAnyNumber") => {
                // The offered cards, fixed before any of them is cast.
                let offered =
                    self.resolution_cast_offered(effect, stack_object, bindings, decisions);
                if let Some(binding) = effect["bind"].as_str() {
                    // Bound even when nothing is cast: a later `that weren't
                    // cast` subtracts an empty set, never a missing binding.
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(Vec::new()));
                }
                self.cast_cards_during_resolution(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
                if let Some(binding) = effect["bind"].as_str() {
                    // A cast card is the spell on the stack (CR 601.2a); the
                    // spells of this resolution keep their card identities.
                    let cast = self
                        .state
                        .stack
                        .iter()
                        .map(|object| object.card.instance_id.clone())
                        .filter(|instance_id| offered.contains(instance_id))
                        .collect::<Vec<_>>();
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(cast));
                }
            }
            Some("castParadigmCopy") => {
                self.cast_paradigm_copy(stack_object, provider)?;
            }
            Some("attachPermanent") => {
                let attachment_ids = match value_kind(&effect["attachment"]) {
                    Some("self" | "abilitySource") => {
                        vec![stack_object.card.instance_id.clone()]
                    }
                    _ => self
                        .resolve_runtime_object_ids(
                            &effect["attachment"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .or_else(|| {
                            self.resolve_target(&effect["attachment"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                                    _ => None,
                                })
                        })
                        .unwrap_or_default(),
                };
                if attachment_ids.is_empty() {
                    return Ok(());
                }
                let instance_id = self
                    .resolve_runtime_object_ids(&effect["to"], stack_object, bindings, decisions)
                    .and_then(|objects| objects.into_iter().next())
                    .or_else(|| {
                        self.resolve_target(&effect["to"], stack_object, bindings)
                            .and_then(|target| match target {
                                TargetRef::Permanent { instance_id } => Some(instance_id),
                                _ => None,
                            })
                    });
                let Some(instance_id) = instance_id else {
                    return Ok(());
                };
                let Some((recipient_player, recipient_index)) =
                    self.permanent_position(&instance_id)
                else {
                    return Ok(());
                };
                let recipient_is_creature = self.permanent_is_creature(
                    &self.state.players[recipient_player].battlefield[recipient_index],
                );
                let recipient_is_land = self.permanent_has_card_type(
                    &self.state.players[recipient_player].battlefield[recipient_index],
                    "Land",
                );
                let mut attached_ids = Vec::new();
                for attachment_id in attachment_ids {
                    if let Some((player_index, card_index)) =
                        self.permanent_position(&attachment_id)
                    {
                        // CR 701.3b/701.3c: an object that cannot legally be
                        // attached to the recipient stays where it is, and an
                        // object never becomes attached to itself. Only an
                        // Aura, Equipment or Fortification can be attached; an
                        // Equipment only to a creature (CR 301.5c) and a
                        // Fortification only to a land (CR 301.6).
                        let type_line = &self.state.players[player_index].battlefield[card_index]
                            .definition
                            .type_line;
                        let attachable = type_line_contains(type_line, "Aura")
                            || (type_line_contains(type_line, "Equipment")
                                && recipient_is_creature)
                            || (type_line_contains(type_line, "Fortification")
                                && recipient_is_land);
                        if attachment_id == instance_id || !attachable {
                            continue;
                        }
                        self.state.players[player_index].battlefield[card_index].attached_to =
                            Some(instance_id.clone());
                        self.record_event(
                            "permanentAttached",
                            Some(stack_object.controller.clone()),
                            Some(attachment_id.clone()),
                            json!({ "attachedTo": instance_id }),
                        );
                        self.apply_stored_copy_aura(&attachment_id);
                        attached_ids.push(attachment_id);
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(attached_ids));
                }
                self.refresh_control_effects();
            }
            Some("castOneCard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let card_ids = self
                    .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                let source_zone = effect["sourceZone"].as_str().unwrap_or("graveyard");
                let zone = match source_zone {
                    "graveyard" => &self.state.players[player_index].graveyard,
                    "hand" => &self.state.players[player_index].hand,
                    "exile" => &self.state.players[player_index].exile,
                    _ => return Ok(()),
                };
                let cards = zone
                    .iter()
                    .filter(|card| card_ids.contains(&card.instance_id))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut options = self.cast_one_card_actions(
                    player_index,
                    &cards,
                    source_zone,
                    effect["normalCostsApply"].as_bool() == Some(true),
                    effect.get("spellWhere"),
                )?;
                // An absent hit or a card with no legal resulting spell
                // does not ask the player to decline an impossible cast.
                if options.is_empty() {
                    return Ok(());
                }
                if effect["optional"].as_bool() == Some(true) {
                    options.push(LegalAction {
                        id: format!("declineCastOne:{}", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: "Do not cast a spell".to_string(),
                        card_instance_id: None,
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    });
                }
                if !options.is_empty() {
                    let action = self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("castOne:{}", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id,
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options,
                        },
                    )?;
                    if action.kind == ActionKind::CastSpell {
                        let cast_card_id = action.card_instance_id.clone();
                        self.apply_priority_action(&action, provider)?;
                        if let (Some(binding_id), Some(cast_card_id)) =
                            (effect["bind"].as_str(), cast_card_id)
                        {
                            bindings.insert(
                                binding_id.to_string(),
                                RuntimeBinding::Objects(vec![cast_card_id]),
                            );
                        }
                    }
                }
            }
            Some("counterSpell") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["spell"], stack_object, bindings)
                {
                    let target = self
                        .state
                        .stack
                        .iter()
                        .find(|object| object.id == stack_id)
                        .cloned();
                    let exile_instead = effect["exileInstead"].as_bool() == Some(true)
                        || target.as_ref().is_some_and(|target| {
                            effect.get("exileInsteadWhere").is_some_and(|filter| {
                                matches_card_filter(&target.card.definition, filter)
                            })
                        });
                    self.counter_spell(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        exile_instead,
                    )?;
                    let countered = !self.state.stack.iter().any(|object| object.id == stack_id);
                    if countered && let Some(target) = target {
                        if exile_instead && let Some(binding_id) = effect["bindExiledAs"].as_str() {
                            bindings.insert(
                                binding_id.to_string(),
                                RuntimeBinding::Objects(vec![target.card.instance_id.clone()]),
                            );
                        }
                        if let Some(tokens) = effect.get("controllerCreatesTokens") {
                            let mut token_stack = stack_object.clone();
                            token_stack.controller = target.controller.clone();
                            self.create_tokens_with_provider(
                                &json!({
                                    "kind": "createTokens",
                                    "controller": { "kind": "abilityController" },
                                    "quantity": tokens["quantity"],
                                    "token": { "kind": "namedToken", "name": tokens["name"] },
                                }),
                                &token_stack,
                                bindings,
                                decisions,
                                provider,
                            )?;
                        }
                        if effect["prohibitControllerSpellsThisTurn"].as_bool() == Some(true) {
                            self.state.rule_modifiers.push(json!({
                                "kind": "prohibitAdditionalSpells",
                                "playerId": target.controller,
                                "expiresAfterTurn": self.state.turn_number,
                            }));
                        }
                        if effect["addManaAtNextMainPhase"].as_str() == Some("C") {
                            let amount = mana_value_with_decisions(
                                &target.card.definition,
                                &target.decisions,
                            );
                            let active_player_id =
                                self.state.players[self.state.active_player].id.as_str();
                            let before_first_main = active_player_id == stack_object.controller
                                && matches!(
                                    self.state.step,
                                    GameStep::Untap | GameStep::Upkeep | GameStep::Draw
                                );
                            let earliest_turn = if before_first_main {
                                self.state.turn_number
                            } else {
                                self.state.turn_number.saturating_add(1)
                            };
                            self.state.rule_modifiers.push(json!({
                                "kind": "plasmCaptureMana",
                                "playerId": stack_object.controller,
                                "amount": amount,
                                "earliestTurn": earliest_turn,
                                "sourceCard": stack_object.card,
                            }));
                        }
                    }
                }
            }
            Some("counterStackObject") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    self.counter_stack_object(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        effect["exileInstead"].as_bool().unwrap_or(false),
                    )?;
                }
            }
            Some("counterOpponentStackObjects") => {
                let stack_ids = self
                    .state
                    .stack
                    .iter()
                    .filter(|object| object.controller != stack_object.controller)
                    .map(|object| object.id.clone())
                    .collect::<Vec<_>>();
                let mut countered = 0;
                for stack_id in stack_ids {
                    let was_present = self.state.stack.iter().any(|object| object.id == stack_id);
                    self.counter_stack_object(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        false,
                    )?;
                    if was_present && !self.state.stack.iter().any(|object| object.id == stack_id) {
                        countered += 1;
                    }
                }
                if let Some(binding_id) = effect["bindCountAs"].as_str() {
                    bindings.insert(binding_id.to_string(), RuntimeBinding::Integer(countered));
                }
            }
            Some("suspendStackItem") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    let time_counters = integer_value(&effect["timeCounters"]).unwrap_or(1);
                    self.suspend_stack_object(&stack_id, time_counters)?;
                }
            }
            Some("changeSingleTarget") => {
                let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((target_id, original_target)) = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .and_then(|object| object.targets.iter().next())
                    .map(|(id, target)| (id.clone(), target.clone()))
                else {
                    return Ok(());
                };
                let candidates = match original_target {
                    TargetRef::Player { .. } => json!({ "kind": "players" }),
                    TargetRef::Permanent { .. } => {
                        json!({ "kind": "permanents", "where": Value::Null })
                    }
                    TargetRef::StackObject { .. } => {
                        json!({ "kind": "stackItems", "where": Value::Null })
                    }
                    TargetRef::Card { .. } => return Ok(()),
                };
                let Some(new_target) = self.choose_resolution_target(
                    "newTarget",
                    &candidates,
                    stack_object,
                    false,
                    provider,
                )?
                else {
                    return Ok(());
                };
                let affected_controller = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .map(|object| object.controller.clone());
                if let Some(object) = self
                    .state
                    .stack
                    .iter_mut()
                    .find(|object| object.id == stack_id)
                {
                    object.targets.insert(target_id, new_target.clone());
                    self.record_event(
                        "stackObjectTargetChanged",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({
                            "stackId": stack_id,
                            "target": new_target,
                            "affectedControllerId": affected_controller,
                        }),
                    );
                }
            }
            Some("changeTargets") => {
                self.execute_target_reassignment(effect, stack_object, bindings, provider)?;
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

    /// The zones a resolution-time cast reads its cards from.
    fn resolution_cast_zones(effect: &Value) -> Vec<String> {
        match effect.get("sourceZones").and_then(Value::as_array) {
            Some(zones) => zones
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
            None => vec![effect["sourceZone"].as_str().unwrap_or("exile").to_string()],
        }
    }

    /// The cards a resolution-time cast offers, fixed before any of them is
    /// cast: its object expression, or the caster's own cards in their
    /// command zone matching `cardsWhere` (CR 903.3: a commander they own).
    fn resolution_cast_offered(
        &self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> BTreeSet<String> {
        if effect.get("cards").is_some() {
            return self
                .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                .unwrap_or_default()
                .into_iter()
                .collect();
        }
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return BTreeSet::new();
        };
        let Ok(player_index) = self.player_index(&player_id) else {
            return BTreeSet::new();
        };
        self.state.players[player_index]
            .command_zone
            .iter()
            .filter(|card| {
                card.owner == player_id
                    && matches_card_filter(&card.definition, &effect["cardsWhere"])
            })
            .map(|card| card.instance_id.clone())
            .collect()
    }

    /// The commander tax of a cast from the command zone during resolution
    /// (CR 903.8): an additional cost paid even without the mana cost.
    pub(in super::super) fn resolution_cast_command_tax(
        &self,
        card: &CardInstance,
        source_zone: &str,
    ) -> Vec<String> {
        if source_zone != "commandZone" {
            return Vec::new();
        }
        let tax = self
            .state
            .commanders
            .iter()
            .find(|commander| commander.card_instance_id == card.instance_id)
            .map(|commander| commander.casts_from_command_zone.saturating_mul(2))
            .unwrap_or(0);
        (tax > 0)
            .then(|| format!("{{{tax}}}"))
            .into_iter()
            .collect()
    }

    /// The land plays a `you may play` offers while an effect resolves: a
    /// land among the cards is played only during its player's own turn and
    /// only with a land play left (CR 305.2, 305.3), from the zone it is in.
    fn resolution_land_play_actions(
        &self,
        player_index: usize,
        cards: &[CardInstance],
        source_zone: &str,
    ) -> Vec<LegalAction> {
        let player_id = self.state.players[player_index].id.clone();
        if self.state.players[self.state.active_player].id != player_id {
            return Vec::new();
        }
        let played = self
            .state
            .events
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == "landPlayed"
                    && event.player_id.as_deref() == Some(player_id.as_str())
            })
            .count();
        let allowed = 1_i64
            .saturating_add(i64::from(self.additional_land_play_count(&player_id)))
            .max(0);
        if i64::try_from(played).unwrap_or(i64::MAX) >= allowed {
            return Vec::new();
        }
        cards
            .iter()
            .filter(|card| is_land(&card.definition))
            .filter(|card| {
                !self
                    .battlefield_entry_admission_for_id(&card.instance_id, source_zone)
                    .is_prohibited()
            })
            .map(|card| LegalAction {
                id: format!("playLand:duringResolution:{}", card.instance_id),
                kind: ActionKind::PlayLand,
                player_id: player_id.clone(),
                label: format!("Play {}", card.definition.name),
                card_instance_id: Some(card.instance_id.clone()),
                payment_sources: Vec::new(),
                decisions: BTreeMap::from([(
                    "castSourceZone".to_string(),
                    Value::String(source_zone.to_string()),
                )]),
                targets: BTreeMap::new(),
                target_order: Vec::new(),
                attacker_id: None,
                blocker_id: None,
            })
            .collect()
    }

    /// CR 608.2g: the cards are cast while this effect resolves, from the
    /// zones they are in, without their mana cost, at their printed cost,
    /// with the player-counter or life alternative cost (CR 118.9), and a
    /// land among them is played when the effect says `play` (CR 305.3).
    /// An optional cast offers a finishing choice; a `mandatory` cast (an
    /// imperative `cast`, performed if able) offers it only once no legal
    /// cast remains or the maximum is reached. Each spell cast this way
    /// carries the stated graveyard exile replacement.
    fn cast_cards_during_resolution<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(());
        };
        let player_index = self.player_index(&player_id)?;
        let object_ids = self.resolution_cast_offered(effect, stack_object, bindings, decisions);
        let source_zones = Self::resolution_cast_zones(effect);
        let maximum =
            integer_value(&effect["maximum"]).and_then(|value| usize::try_from(value).ok());
        let alternative_counter = (value_kind(&effect["alternativeCost"])
            == Some("payPlayerCounters"))
        .then(|| effect["alternativeCost"]["counter"].as_str())
        .flatten();
        let alternative_life = value_kind(&effect["alternativeCost"]) == Some("payLife");
        let play_lands = effect["playLands"].as_bool() == Some(true);
        let mut cast_count = 0;
        let mut performed = 0;
        loop {
            if maximum.is_some_and(|maximum| performed >= maximum) {
                break;
            }
            let normal_costs_apply = effect["withoutPayingManaCost"].as_bool() == Some(false)
                && alternative_counter.is_none()
                && !alternative_life;
            let mut offered_cards = Vec::new();
            let mut options = Vec::new();
            for source_zone in &source_zones {
                let cards = match source_zone.as_str() {
                    "hand" | "graveyard" | "exile" | "commandZone" => self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| match source_zone.as_str() {
                            "hand" => player.hand.iter(),
                            "graveyard" => player.graveyard.iter(),
                            "exile" => player.exile.iter(),
                            _ => player.command_zone.iter(),
                        })
                        .filter(|card| object_ids.contains(&card.instance_id))
                        .cloned()
                        .collect::<Vec<_>>(),
                    _ => continue,
                };
                if let Some(counter) = alternative_counter {
                    let available = self.state.players[player_index]
                        .counters
                        .get(counter)
                        .copied()
                        .unwrap_or(0);
                    for card in cards
                        .iter()
                        .filter(|card| mana_value(&card.definition) <= available)
                    {
                        if !self.state.permissions.iter().any(|permission| {
                            value_kind(permission)
                                == Some("resolutionPlayerCounterAlternativePermission")
                                && permission["cardInstanceId"].as_str()
                                    == Some(card.instance_id.as_str())
                                && permission["playerId"].as_str() == Some(player_id.as_str())
                                && permission["sourceStackId"].as_str()
                                    == Some(stack_object.id.as_str())
                        }) {
                            self.state.permissions.push(json!({
                                "kind": "resolutionPlayerCounterAlternativePermission",
                                "cardInstanceId": card.instance_id,
                                "playerId": player_id,
                                "sourceStackId": stack_object.id,
                                "counter": counter,
                            }));
                        }
                    }
                }
                options.extend(self.cast_actions_from_zone(
                    player_index,
                    &cards,
                    "duringResolution",
                    source_zone,
                    normal_costs_apply,
                )?);
                if play_lands {
                    options.extend(self.resolution_land_play_actions(
                        player_index,
                        &cards,
                        source_zone,
                    ));
                }
                offered_cards.extend(cards);
            }
            let card_of = |action: &LegalAction| {
                action
                    .card_instance_id
                    .as_deref()
                    .and_then(|instance_id| {
                        offered_cards
                            .iter()
                            .find(|card| card.instance_id == instance_id)
                    })
                    .cloned()
            };
            if let Some(counter) = alternative_counter {
                let available = self.state.players[player_index]
                    .counters
                    .get(counter)
                    .copied()
                    .unwrap_or(0);
                options.retain(|action| {
                    card_of(action).is_some_and(|card| mana_value(&card.definition) <= available)
                });
                for action in &mut options {
                    action.decisions.remove("withoutPayingManaCost");
                    action.decisions.insert(
                        "useAlternativePlayerCounterCost".to_string(),
                        Value::Bool(true),
                    );
                    action.decisions.insert(
                        "alternativePlayerCounter".to_string(),
                        Value::String(counter.to_string()),
                    );
                    if let Some(card) = card_of(action) {
                        action.label = format!(
                            "Cast {} by paying {} {} counter(s)",
                            card.definition.name,
                            mana_value(&card.definition),
                            counter
                        );
                    }
                }
            }
            if alternative_life {
                // CR 118.9, 119.4: life equal to the cast card's mana value,
                // with the spell's other life costs; a player pays life only
                // with a life total at least that amount.
                let life = self.state.players[player_index].life;
                let mut admitted = Vec::with_capacity(options.len());
                for mut action in options {
                    let Some(card) = card_of(&action) else {
                        continue;
                    };
                    let required = mana_value(&card.definition).max(0);
                    let total = action
                        .decisions
                        .get("castingLifePayment")
                        .and_then(integer_value)
                        .unwrap_or(0)
                        .max(0)
                        .saturating_add(required);
                    if life < total {
                        continue;
                    }
                    if total > 0 {
                        action
                            .decisions
                            .insert("castingLifePayment".to_string(), Value::from(total));
                    }
                    action.label =
                        format!("Cast {} by paying {} life", card.definition.name, required);
                    admitted.push(action);
                }
                options = admitted;
            }
            if options.is_empty() {
                break;
            }
            if let Some(provenance) = suspend_keyword::cast_via_decision(&effect["castVia"]) {
                for action in &mut options {
                    action
                        .decisions
                        .insert(provenance.to_string(), Value::Bool(true));
                }
            }
            // A mandatory cast leaves no way to decline while a legal cast
            // of an offered card exists (CR 608.2g, `if able`).
            if effect["mandatory"].as_bool() != Some(true) {
                options.push(LegalAction {
                    id: format!("finishCasting:{}", stack_object.id),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: "Finish casting spells".to_string(),
                    card_instance_id: None,
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
            let action = self.choose_action(
                provider,
                EngineDecisionRequest {
                    id: format!("castAnyNumber:{}:{performed}", stack_object.id),
                    kind: DecisionKind::ResolutionChoice,
                    player_id: player_id.clone(),
                    source_card: Some(stack_object.card.clone()),
                    source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                    choice: None,
                    options,
                },
            )?;
            if action.kind == ActionKind::ChooseResolution {
                break;
            }
            let land_play = play_lands && action.kind == ActionKind::PlayLand;
            if action.kind != ActionKind::CastSpell && !land_play {
                return Err(EngineError::new(
                    "cast-any-number decision returned an invalid action",
                ));
            }
            let cast_card_id = action.card_instance_id.clone();
            self.apply_priority_action(&action, provider)?;
            performed += 1;
            if !land_play {
                cast_count += 1;
                self.install_cast_graveyard_exile(effect, stack_object, cast_card_id.as_deref());
            }
            if self.state.outcome.is_some() {
                break;
            }
        }
        decisions.insert(
            "spellsCastWithoutPaying".to_string(),
            Value::from(cast_count),
        );
        self.state.permissions.retain(|permission| {
            value_kind(permission) != Some("resolutionPlayerCounterAlternativePermission")
                || permission["sourceStackId"].as_str() != Some(stack_object.id.as_str())
        });
        Ok(())
    }

    /// `If that spell would be put into a graveyard, exile it instead` (CR
    /// 614.1a) on one spell this cast put on the stack. A card keeps its
    /// identity as it becomes the spell (CR 601.2a). A nonpermanent spell
    /// carries the one-shot replacement of its graveyard destination, which
    /// the shared graveyard pipeline applies wherever the spell goes there
    /// (resolution, counter, fizzle). A permanent spell that resolves becomes
    /// a new object (CR 400.7) the replacement must not follow, so it is
    /// marked to be exiled when it leaves the stack other than by resolving;
    /// a spell copy ceases to exist instead (CR 707.10a).
    fn install_cast_graveyard_exile(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        cast_card_id: Option<&str>,
    ) {
        let replacement = &effect["exileInsteadOfGraveyard"];
        if !replacement.is_object() {
            return;
        }
        let Some(card_id) = cast_card_id else {
            return;
        };
        let Some(index) = self
            .state
            .stack
            .iter()
            .position(|object| object.card.instance_id == card_id)
        else {
            return;
        };
        let spell = &self.state.stack[index];
        if is_spell_copy(&spell.card)
            || (replacement["graveyard"].as_str() == Some("controller")
                && spell.card.owner != stack_object.controller)
            || replacement
                .get("where")
                .is_some_and(|filter| !matches_card_filter(&spell.card.definition, filter))
        {
            return;
        }
        if is_permanent(&spell.card.definition) {
            self.state.stack[index].exile_on_leave_stack = true;
            return;
        }
        self.state.rule_modifiers.push(json!({
            "kind": "exileSpecificCardInsteadOfGraveyard",
            "cardInstanceId": card_id,
            "sourceCardInstanceId": stack_object.card.instance_id,
        }));
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "stack_casting_tests.rs"]
mod tests;
