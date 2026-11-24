//! Costs paid with objects chosen while casting or activating (CR 601.2h,
//! 602.2b) that the casting and activation pipelines had no payer for:
//! tapping or returning chosen permanents and revealing chosen cards as a
//! spell's additional cost, exiling a chosen card from the caster's hand or
//! graveyard, and the activation costs `reveal` (the source or chosen cards
//! in hand) and `mill N`.
//!
//! The objects are declared by the shared `chooseObjects` decision of the
//! rule; a cost names its choice by `chosenObject` id (`id` or `id:index`).
//! Every payer re-checks the object against the cost before it moves or
//! changes it: a declaration that became illegal is refused, never paid with
//! another object.
use super::*;

/// The cost kinds this module owns, as `cost_supported` reads them.
pub(super) fn chosen_object_cost_supported(cost: &Value) -> Option<bool> {
    match value_kind(cost) {
        // CR 701.20a: revealing a card from the payer's hand.
        Some("revealCard") => Some(
            value_kind(&cost["card"]) == Some("self") || target_reference_supported(&cost["card"]),
        ),
        // CR 701.43a: exerting the source permanent.
        Some("exert") => Some(value_kind(&cost["object"]) == Some("self")),
        // CR 701.13a: the payer mills N cards (CR 118.3: N must be there).
        Some("millCards") => {
            Some(integer_value(&cost["count"]).is_some_and(|count| (1..=64).contains(&count)))
        }
        _ => None,
    }
}

/// The casting cost kinds whose objects are chosen while casting and paid by
/// this module.
const CASTING_CHOSEN_OBJECT_COSTS: [&str; 4] = [
    "tap",
    "returnPermanentToOwnersHand",
    "revealCard",
    "exileObject",
];

fn chosen_cost_id(cost: &Value) -> Option<&str> {
    match value_kind(cost) {
        Some("tap" | "exileObject") => cost["object"]["id"].as_str(),
        Some("returnPermanentToOwnersHand") => cost["permanent"]["id"].as_str(),
        Some("revealCard") => cost["card"]["id"].as_str(),
        _ => None,
    }
}

impl GameEngine {
    /// The chosen-object costs of the spell's additional costs, through the
    /// branches its announced mode selects.
    fn casting_chosen_object_costs(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
        kinds: &[&str],
    ) -> Vec<Value> {
        fn collect(
            engine: &GameEngine,
            costs: &Value,
            controller_id: &str,
            decisions: &BTreeMap<String, Value>,
            kinds: &[&str],
            selected: &mut Vec<Value>,
        ) {
            for cost in costs.as_array().into_iter().flatten() {
                match value_kind(cost) {
                    Some("conditional") => {
                        let branch =
                            if engine.condition_value(&cost["condition"], controller_id, decisions)
                            {
                                &cost["then"]
                            } else {
                                &cost["else"]
                            };
                        collect(engine, branch, controller_id, decisions, kinds, selected);
                    }
                    Some(kind) if kinds.contains(&kind) && chosen_cost_id(cost).is_some() => {
                        selected.push(cost.clone());
                    }
                    _ => {}
                }
            }
        }
        let mut selected = Vec::new();
        for rule in card
            .printed_rules()
            .iter()
            .filter(|rule| value_kind(rule) == Some("spellAbility"))
        {
            collect(
                self,
                &rule["declaration"]["additionalCosts"],
                controller_id,
                decisions,
                kinds,
                &mut selected,
            );
        }
        selected
    }

    /// The choice ids of the spell's chosen-object costs: payment choices,
    /// never targets of the spell's effects.
    pub(super) fn casting_chosen_object_cost_target_ids(
        &self,
        card: &CardDefinition,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        self.casting_chosen_object_costs(
            card,
            controller_id,
            decisions,
            &CASTING_CHOSEN_OBJECT_COSTS,
        )
        .iter()
        .filter_map(|cost| chosen_cost_id(cost).map(ToOwned::to_owned))
        .collect()
    }

    fn chosen_permanent(&self, action: &LegalAction, id: &str) -> Result<String, EngineError> {
        let instance_id = match action.targets.get(id) {
            Some(TargetRef::Permanent { instance_id }) => instance_id.clone(),
            _ => return Err(EngineError::new("a cost's chosen permanent is missing")),
        };
        let (player_index, card_index) = self
            .permanent_position(&instance_id)
            .ok_or_else(|| EngineError::new("a cost's chosen permanent left the battlefield"))?;
        if self.state.players[player_index].battlefield[card_index].controller != action.player_id {
            return Err(EngineError::new(
                "a cost's chosen permanent is not controlled by the payer",
            ));
        }
        Ok(instance_id)
    }

    fn chosen_hand_card(&self, action: &LegalAction, id: &str) -> Result<String, EngineError> {
        let instance_id = match action.targets.get(id) {
            Some(TargetRef::Card { instance_id }) => instance_id.clone(),
            _ => return Err(EngineError::new("a cost's chosen card is missing")),
        };
        let player_index = self.player_index(&action.player_id)?;
        if !self.state.players[player_index]
            .hand
            .iter()
            .any(|card| card.instance_id == instance_id)
        {
            return Err(EngineError::new(
                "a cost's chosen card left the payer's hand",
            ));
        }
        Ok(instance_id)
    }

    /// Taps one chosen untapped permanent (CR 701.26a).
    fn tap_chosen_permanent_as_cost(
        &mut self,
        action: &LegalAction,
        id: &str,
    ) -> Result<(), EngineError> {
        let instance_id = self.chosen_permanent(action, id)?;
        let tapped = self
            .tap_permanent(&instance_id)?
            .ok_or_else(|| EngineError::new("a tapped permanent cannot pay a tap cost"))?;
        self.enqueue_permanent_tapped_triggers(&tapped);
        Ok(())
    }

    /// Reveals one card of the payer's hand (CR 701.20a): the card stays in
    /// hand and the reveal is recorded for every player.
    fn reveal_hand_card_as_cost(&mut self, player_id: &str, instance_id: &str) {
        self.record_event(
            "cardRevealed",
            Some(player_id.to_string()),
            Some(instance_id.to_string()),
            json!({ "reason": "cost", "zone": "hand" }),
        );
    }

    /// Exiles a chosen card from the payer's hand or graveyard.
    pub(super) fn exile_chosen_card_as_cost(
        &mut self,
        player_id: &str,
        instance_id: &str,
    ) -> Result<(), EngineError> {
        let player_index = self.player_index(player_id)?;
        let player = &self.state.players[player_index];
        let (zone, card_index) = if let Some(index) = player
            .hand
            .iter()
            .position(|card| card.instance_id == instance_id)
        {
            ("hand", index)
        } else if let Some(index) = player
            .graveyard
            .iter()
            .position(|card| card.instance_id == instance_id)
        {
            ("graveyard", index)
        } else {
            return Err(EngineError::new(
                "exile cost card is no longer in the payer's hand or graveyard",
            ));
        };
        let mut exiled = self.take_zone_card_for_movement(player_index, zone, card_index)?;
        let owner_index = self.player_index(&exiled.owner)?;
        exiled.controller = exiled.owner.clone();
        self.put_owned_card_into_exile(owner_index, exiled)?;
        self.record_event(
            "cardExiled",
            Some(player_id.to_string()),
            Some(instance_id.to_string()),
            json!({ "from": zone, "reason": "cost" }),
        );
        if zone == "graveyard" {
            self.enqueue_cards_left_graveyard_triggers(player_id, &[instance_id.to_string()]);
        }
        Ok(())
    }

    /// Pays the spell's tap, return and reveal additional costs with the
    /// objects declared for them. Exile costs are paid with the existing
    /// exile cost pass.
    pub(super) fn apply_casting_chosen_object_costs(
        &mut self,
        action: &LegalAction,
        card: &CardDefinition,
    ) -> Result<(), EngineError> {
        let costs = self.casting_chosen_object_costs(
            card,
            &action.player_id,
            &action.decisions,
            &["tap", "returnPermanentToOwnersHand", "revealCard"],
        );
        // Validate every declared object before the first payment (CR 118.3).
        for cost in &costs {
            let id = chosen_cost_id(cost).unwrap_or_default();
            match value_kind(cost) {
                Some("revealCard") => {
                    self.chosen_hand_card(action, id)?;
                }
                _ => {
                    self.chosen_permanent(action, id)?;
                }
            }
        }
        for cost in &costs {
            let id = chosen_cost_id(cost).unwrap_or_default();
            match value_kind(cost) {
                Some("tap") => self.tap_chosen_permanent_as_cost(action, id)?,
                Some("returnPermanentToOwnersHand") => {
                    let instance_id = self.chosen_permanent(action, id)?;
                    self.return_permanent_to_hand(&instance_id)?;
                }
                Some("revealCard") => {
                    let instance_id = self.chosen_hand_card(action, id)?;
                    self.reveal_hand_card_as_cost(&action.player_id, &instance_id);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The activation's reveal and mill costs can be paid: a revealed source
    /// is in its controller's hand, and the payer's library holds every card
    /// a mill cost names (CR 118.3). Chosen revealed cards are proved by
    /// their declaration.
    pub(super) fn activated_chosen_object_costs_are_payable(
        &self,
        rule: &Value,
        controller_id: &str,
        source_instance_id: &str,
    ) -> bool {
        let Ok(player_index) = self.player_index(controller_id) else {
            return false;
        };
        let player = &self.state.players[player_index];
        let mill = rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| value_kind(cost) == Some("millCards"))
            .filter_map(|cost| integer_value(&cost["count"]))
            .map(i64::from)
            .sum::<i64>();
        let reveals_source = rule["costs"].as_array().into_iter().flatten().any(|cost| {
            value_kind(cost) == Some("revealCard") && value_kind(&cost["card"]) == Some("self")
        });
        usize::try_from(mill).is_ok_and(|mill| player.library.len() >= mill)
            && (!reveals_source
                || player
                    .hand
                    .iter()
                    .any(|card| card.instance_id == source_instance_id))
    }

    /// Pays the activation's reveal and mill costs.
    pub(super) fn apply_activated_chosen_object_costs(
        &mut self,
        action: &LegalAction,
        rule: &Value,
    ) -> Result<(), EngineError> {
        let costs = rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| matches!(value_kind(cost), Some("revealCard" | "millCards" | "exert")))
            .cloned()
            .collect::<Vec<_>>();
        if costs.is_empty() {
            return Ok(());
        }
        let source_id = action
            .card_instance_id
            .clone()
            .ok_or_else(|| EngineError::new("activated cost source is missing"))?;
        let mut revealed = Vec::new();
        let mut mill = 0_usize;
        for cost in &costs {
            match value_kind(cost) {
                Some("revealCard") if value_kind(&cost["card"]) == Some("self") => {
                    let player_index = self.player_index(&action.player_id)?;
                    if !self.state.players[player_index]
                        .hand
                        .iter()
                        .any(|card| card.instance_id == source_id)
                    {
                        return Err(EngineError::new(
                            "a revealed source is no longer in its controller's hand",
                        ));
                    }
                    revealed.push(source_id.clone());
                }
                Some("revealCard") => {
                    let id = cost["card"]["id"]
                        .as_str()
                        .ok_or_else(|| EngineError::new("reveal cost has no card choice"))?;
                    revealed.push(self.chosen_hand_card(action, id)?);
                }
                Some("exert") => {
                    // CR 701.43a: it won't untap during its controller's
                    // next untap step (the shared skip-next-untap rule).
                    if self.permanent_position(&source_id).is_none() {
                        return Err(EngineError::new("an exerted source left the battlefield"));
                    }
                    self.state.rule_modifiers.push(json!({
                        "kind": "skipNextUntap",
                        "sourceCardInstanceId": source_id,
                        "cardInstanceId": source_id,
                    }));
                    self.record_event(
                        "permanentExerted",
                        Some(action.player_id.clone()),
                        Some(source_id.clone()),
                        json!({ "reason": "cost" }),
                    );
                }
                Some("millCards") => {
                    mill += integer_value(&cost["count"])
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| EngineError::new("mill cost has no count"))?;
                }
                _ => {}
            }
        }
        let player_index = self.player_index(&action.player_id)?;
        if self.state.players[player_index].library.len() < mill {
            return Err(EngineError::new(
                "the payer's library cannot pay the mill cost",
            ));
        }
        let mut distinct = revealed.clone();
        distinct.sort();
        distinct.dedup();
        if distinct.len() != revealed.len() {
            return Err(EngineError::new(
                "the same card cannot pay two reveal costs",
            ));
        }
        for instance_id in &revealed {
            self.reveal_hand_card_as_cost(&action.player_id, instance_id);
        }
        if mill > 0 {
            self.mill_cards(&action.player_id, mill, Some(&action.player_id))?;
        }
        Ok(())
    }
}
