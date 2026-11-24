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
        // Every object of one declared choice (`Sacrifice X Goats`).
        Some("chosenObjectsCost") => Some(
            cost["choice"]
                .as_str()
                .is_some_and(|choice| !choice.is_empty())
                && cost["action"]
                    .as_str()
                    .is_some_and(|action| CHOSEN_OBJECTS_ACTIONS.contains(&action))
                && cost
                    .get("from")
                    .is_none_or(|zone| matches!(value_kind(zone), Some("hand" | "graveyard"))),
        ),
        // CR 701.43a: exerting the source permanent.
        Some("exert") => Some(value_kind(&cost["object"]) == Some("self")),
        // CR 701.13a: the payer mills N cards (CR 118.3: N must be there).
        Some("millCards") => {
            Some(integer_value(&cost["count"]).is_some_and(|count| (1..=64).contains(&count)))
        }
        // CR 701.9a: the payer discards every card in their hand; an empty
        // hand pays it.
        Some("discardHand") => Some(cost.get("player").is_none_or(player_reference_supported)),
        // CR 401.2: the payer's top N library cards are exiled (all N must
        // be there, CR 118.3).
        Some("exileTopLibraryCards") => Some(
            cost.get("player").is_none_or(player_reference_supported)
                && integer_value(&cost["count"]).is_some_and(|count| (1..=64).contains(&count)),
        ),
        // CR 301.5a, 303.4a: the permanent the source is attached to.
        Some(ATTACHED_TAP_COST | ATTACHED_SACRIFICE_COST) => Some(true),
        _ => None,
    }
}

/// `tap enchanted|equipped NOUN` as a cost.
const ATTACHED_TAP_COST: &str = "tapAttachedPermanent";
/// `sacrifice enchanted|equipped NOUN` as a cost.
const ATTACHED_SACRIFICE_COST: &str = "sacrificeAttachedPermanent";

/// The designated-object costs this module pays without a declaration.
const DESIGNATED_COSTS: [&str; 4] = [
    "discardHand",
    "exileTopLibraryCards",
    ATTACHED_TAP_COST,
    ATTACHED_SACRIFICE_COST,
];

fn designated_library_cards(rule: &Value) -> i64 {
    rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cost| matches!(value_kind(cost), Some("millCards" | "exileTopLibraryCards")))
        .filter_map(|cost| integer_value(&cost["count"]))
        .map(i64::from)
        .sum::<i64>()
}

/// The quantity of a `chooseObjects` declaration: an exact positive literal,
/// or exactly the value of an earlier number decision of the same
/// declaration (`Sacrifice X creatures`, CR 107.3a).
pub(super) fn chosen_quantity_supported(quantity: &Value) -> bool {
    value_kind(quantity) == Some("exactly")
        && (integer_value(&quantity["value"]).is_some_and(|value| value > 0)
            || (value_kind(&quantity["value"]) == Some("decisionResult")
                && quantity["value"]["decisionId"].is_string()))
}

/// The number of objects a `chooseObjects` declaration chooses under the
/// decisions made before it. X may be zero (no object is chosen).
pub(super) fn declared_chosen_quantity(
    quantity: &Value,
    decisions: &BTreeMap<String, Value>,
) -> Option<i32> {
    if value_kind(quantity) != Some("exactly") {
        return None;
    }
    if let Some(value) = integer_value(&quantity["value"]) {
        return (value > 0).then_some(value);
    }
    (value_kind(&quantity["value"]) == Some("decisionResult"))
        .then(|| quantity["value"]["decisionId"].as_str())
        .flatten()
        .and_then(|id| decisions.get(id))
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value >= 0)
}

/// The actions a `chosenObjectsCost` performs on each object of its choice.
const CHOSEN_OBJECTS_ACTIONS: [&str; 6] = [
    "sacrifice",
    "tap",
    "returnToOwnersHand",
    "discard",
    "exile",
    "reveal",
];

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
        // Milled and exiled top cards both come from the payer's library.
        let mill = designated_library_cards(rule);
        let reveals_source = rule["costs"].as_array().into_iter().flatten().any(|cost| {
            value_kind(cost) == Some("revealCard") && value_kind(&cost["card"]) == Some("self")
        });
        usize::try_from(mill).is_ok_and(|mill| player.library.len() >= mill)
            && (!reveals_source
                || player
                    .hand
                    .iter()
                    .any(|card| card.instance_id == source_instance_id))
            && self.attached_costs_are_payable(rule, controller_id, source_instance_id)
    }

    /// The permanent the source is attached to (CR 301.5a, 303.4a), if any.
    fn cost_source_attachment(&self, source_instance_id: &str) -> Option<String> {
        let (player_index, card_index) = self.permanent_position(source_instance_id)?;
        self.state.players[player_index].battlefield[card_index]
            .attached_to
            .clone()
    }

    /// An attached-permanent cost needs that permanent on the battlefield,
    /// controlled by the payer, untapped for a tap cost and sacrificeable
    /// for a sacrifice cost (CR 118.3, 701.21a).
    fn attached_costs_are_payable(
        &self,
        rule: &Value,
        controller_id: &str,
        source_instance_id: &str,
    ) -> bool {
        let costs = rule["costs"].as_array().into_iter().flatten();
        let mut kinds = costs
            .filter_map(|cost| value_kind(cost))
            .filter(|kind| matches!(*kind, ATTACHED_TAP_COST | ATTACHED_SACRIFICE_COST))
            .peekable();
        if kinds.peek().is_none() {
            return true;
        }
        let Some(attached) = self.cost_source_attachment(source_instance_id) else {
            return false;
        };
        let Some((player_index, card_index)) = self.permanent_position(&attached) else {
            return false;
        };
        let permanent = &self.state.players[player_index].battlefield[card_index];
        permanent.controller == controller_id
            && kinds.all(|kind| match kind {
                ATTACHED_TAP_COST => !permanent.tapped,
                _ => self.permanent_can_be_sacrificed(permanent),
            })
    }

    /// Pays the designated-object costs: the payer's whole hand, the payer's
    /// top library cards, and the permanent the source is attached to. Each
    /// object is re-read at payment; a missing one refuses the payment.
    fn pay_designated_object_costs(
        &mut self,
        action: &LegalAction,
        costs: &[Value],
        source_id: &str,
    ) -> Result<(), EngineError> {
        let player_index = self.player_index(&action.player_id)?;
        for cost in costs {
            match value_kind(cost) {
                Some("discardHand") => {
                    // CR 701.9a: every card in the hand when the cost is paid.
                    let hand = self.state.players[player_index]
                        .hand
                        .iter()
                        .map(|card| card.instance_id.clone())
                        .collect::<Vec<_>>();
                    for instance_id in hand {
                        self.discard_card_as_cost(&action.player_id, &instance_id)?;
                    }
                }
                Some("exileTopLibraryCards") => {
                    let count = integer_value(&cost["count"])
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| EngineError::new("library exile cost has no count"))?;
                    if self.state.players[player_index].library.len() < count {
                        return Err(EngineError::new(
                            "the payer's library cannot pay the exile cost",
                        ));
                    }
                    for _ in 0..count {
                        // The library's top card is its last element.
                        let top = self.state.players[player_index].library.len() - 1;
                        let card =
                            self.take_zone_card_for_movement(player_index, "library", top)?;
                        self.put_owned_card_into_exile(player_index, card)?;
                    }
                }
                Some(kind @ (ATTACHED_TAP_COST | ATTACHED_SACRIFICE_COST)) => {
                    let attached = self.cost_source_attachment(source_id).ok_or_else(|| {
                        EngineError::new("the cost source is attached to no permanent")
                    })?;
                    let (owner_index, card_index) =
                        self.permanent_position(&attached).ok_or_else(|| {
                            EngineError::new("the attached cost permanent left the battlefield")
                        })?;
                    let permanent = &self.state.players[owner_index].battlefield[card_index];
                    if permanent.controller != action.player_id {
                        return Err(EngineError::new(
                            "the attached permanent is not controlled by the payer",
                        ));
                    }
                    if kind == ATTACHED_TAP_COST {
                        let tapped = self.tap_permanent(&attached)?.ok_or_else(|| {
                            EngineError::new("a tapped permanent cannot pay a tap cost")
                        })?;
                        self.enqueue_permanent_tapped_triggers(&tapped);
                    } else {
                        self.move_permanent_to_graveyard(&attached, "sacrificed")?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Every object of one declared choice (`id` or `id:index`), each once.
    fn declared_choice_objects(action: &LegalAction, choice: &str) -> Vec<TargetRef> {
        let prefix = format!("{choice}:");
        let mut seen = BTreeSet::new();
        action
            .targets
            .iter()
            .filter(|(id, _)| id.as_str() == choice || id.starts_with(&prefix))
            .filter(|(_, target)| seen.insert(format!("{target:?}")))
            .map(|(_, target)| target.clone())
            .collect()
    }

    /// Performs a `chosenObjectsCost` action on every chosen object, after
    /// checking each one is still payable (CR 118.3).
    fn pay_chosen_objects_cost(
        &mut self,
        action: &LegalAction,
        cost: &Value,
    ) -> Result<(), EngineError> {
        let choice = cost["choice"]
            .as_str()
            .ok_or_else(|| EngineError::new("a chosen-objects cost has no choice"))?;
        let kind = cost["action"].as_str().unwrap_or_default();
        let objects = Self::declared_choice_objects(action, choice);
        let mut permanents = Vec::new();
        let mut cards = Vec::new();
        for object in &objects {
            match (kind, object) {
                (
                    "sacrifice" | "tap" | "returnToOwnersHand",
                    TargetRef::Permanent { instance_id },
                ) => {
                    let (player_index, card_index) =
                        self.permanent_position(instance_id).ok_or_else(|| {
                            EngineError::new("a chosen cost permanent left the battlefield")
                        })?;
                    let permanent = &self.state.players[player_index].battlefield[card_index];
                    if permanent.controller != action.player_id
                        || (kind == "tap" && permanent.tapped)
                    {
                        return Err(EngineError::new(
                            "a chosen permanent can no longer pay its cost",
                        ));
                    }
                    permanents.push(instance_id.clone());
                }
                ("discard" | "exile" | "reveal", TargetRef::Card { instance_id }) => {
                    cards.push(instance_id.clone());
                }
                _ => return Err(EngineError::new("a chosen cost object has the wrong kind")),
            }
        }
        for instance_id in permanents {
            match kind {
                "sacrifice" => self.move_permanent_to_graveyard(&instance_id, "sacrificed")?,
                "tap" => {
                    let tapped = self.tap_permanent(&instance_id)?.ok_or_else(|| {
                        EngineError::new("a tapped permanent cannot pay a tap cost")
                    })?;
                    self.enqueue_permanent_tapped_triggers(&tapped);
                }
                _ => self.return_permanent_to_hand(&instance_id)?,
            }
        }
        for instance_id in cards {
            match kind {
                "discard" => self.discard_card_as_cost(&action.player_id, &instance_id)?,
                "exile" => self.exile_chosen_card_as_cost(&action.player_id, &instance_id)?,
                _ => {
                    let player_index = self.player_index(&action.player_id)?;
                    if !self.state.players[player_index]
                        .hand
                        .iter()
                        .any(|card| card.instance_id == instance_id)
                    {
                        return Err(EngineError::new("a revealed card left the payer's hand"));
                    }
                    self.reveal_hand_card_as_cost(&action.player_id, &instance_id);
                }
            }
        }
        Ok(())
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
            .filter(|cost| {
                matches!(
                    value_kind(cost),
                    Some("revealCard" | "millCards" | "exert" | "chosenObjectsCost")
                ) || value_kind(cost).is_some_and(|kind| DESIGNATED_COSTS.contains(&kind))
            })
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
        for cost in costs
            .iter()
            .filter(|cost| value_kind(cost) == Some("chosenObjectsCost"))
        {
            self.pay_chosen_objects_cost(action, cost)?;
        }
        self.pay_designated_object_costs(action, &costs, &source_id)?;
        if mill > 0 {
            self.mill_cards(&action.player_id, mill, Some(&action.player_id))?;
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "chosen_object_costs_designated_tests.rs"]
mod designated_tests;
