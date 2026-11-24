//! Resolution-time life, sacrifice, discard, exile and return costs with an
//! explicit payer.
//! This module prepares payment alternatives only; its caller owns decline,
//! provider choice, Boolean results and the enclosing resolution checkpoint.
use super::*;

fn card_cost_option(
    source: &StackObject,
    player_id: &str,
    suffix: &str,
    label: String,
    metadata_key: &str,
    metadata: Value,
) -> LegalAction {
    LegalAction {
        id: format!("resolutionCost:{}:{suffix}", source.id),
        kind: ActionKind::ChooseResolution,
        player_id: player_id.to_string(),
        label,
        card_instance_id: Some(source.card.instance_id.clone()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::from([(metadata_key.to_string(), metadata)]),
        // Objects selected to pay a cost are never Magic targets.
        targets: BTreeMap::new(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    }
}

impl GameEngine {
    pub(in crate::engine) fn card_cost_reference_id(
        &self,
        reference: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Option<String>, EngineError> {
        if reference.is_null() {
            return Ok(None);
        }
        if matches!(value_kind(reference), Some("self" | "abilitySource")) {
            return Ok(Some(source.card.instance_id.clone()));
        }
        match self.resolve_target(reference, source, bindings) {
            Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                Ok(Some(instance_id))
            }
            _ => Err(EngineError::new(
                "card cost object reference is unresolved or is not a card object",
            )),
        }
    }

    pub(in crate::engine) fn prepare_card_cost_options(
        &self,
        cost: &Value,
        player_id: &str,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Vec<LegalAction>, EngineError> {
        let player_index = self.player_index(player_id)?;
        match value_kind(cost) {
            Some("payLife") => {
                let amount = self.checked_runtime_numeric_expression(
                    &cost["amount"],
                    source,
                    bindings,
                    decisions,
                    "life payment amount",
                )?;
                if amount < 0 {
                    return Err(EngineError::new("life payment amount cannot be negative"));
                }
                if !self.player_can_pay_life_cost(player_id, amount) {
                    return Ok(Vec::new());
                }
                Ok(vec![card_cost_option(
                    source,
                    player_id,
                    "life",
                    format!("Pay {amount} life"),
                    "costLifePayment",
                    json!(amount),
                )])
            }
            Some("sacrificePermanent") => {
                let required = self.card_cost_reference_id(&cost["permanent"], source, bindings)?;
                if cost.get("where").is_none() && required.is_none() {
                    return Err(EngineError::new(
                        "sacrifice cost requires a supported filter or object reference",
                    ));
                }
                if !cost["where"].is_null() && !card_filter_supported(&cost["where"]) {
                    return Err(EngineError::new("unsupported sacrifice cost filter"));
                }
                Ok(self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        permanent.controller == player_id
                            && !permanent.flags.get("phasedOut").copied().unwrap_or(false)
                            && self.permanent_can_be_sacrificed(permanent)
                            // `sacrifice another …`: the source object is excluded.
                            && !(cost["excludeSource"] == true
                                && permanent.instance_id == source.card.instance_id)
                            && required
                                .as_ref()
                                .is_none_or(|id| id == &permanent.instance_id)
                            && (cost["where"].is_null()
                                || self.permanent_matches_target_filter_with_decisions(
                                    permanent,
                                    &cost["where"],
                                    player_id,
                                    decisions,
                                ))
                    })
                    .map(|permanent| {
                        card_cost_option(
                            source,
                            player_id,
                            &format!("sacrifice:{}", permanent.instance_id),
                            format!("Sacrifice {}", permanent.definition.name),
                            "costSacrificePermanent",
                            json!(permanent.instance_id),
                        )
                    })
                    .collect())
            }
            // `return a QUERY you control to its owner's hand` (CR 118.3):
            // one permanent the payer controls, chosen while paying.
            Some("returnPermanentToOwnersHand") => {
                if !cost["where"].is_null() && !card_filter_supported(&cost["where"]) {
                    return Err(EngineError::new("unsupported return cost filter"));
                }
                Ok(self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        self.permanent_can_pay_return_cost(permanent, player_id, cost, source)
                            && (cost["where"].is_null()
                                || self.permanent_matches_target_filter_with_decisions(
                                    permanent,
                                    &cost["where"],
                                    player_id,
                                    decisions,
                                ))
                    })
                    .map(|permanent| {
                        card_cost_option(
                            source,
                            player_id,
                            &format!("return:{}", permanent.instance_id),
                            format!("Return {} to its owner's hand", permanent.definition.name),
                            "costReturnPermanent",
                            json!(permanent.instance_id),
                        )
                    })
                    .collect())
            }
            Some("discardCard") => {
                let required = self.card_cost_reference_id(&cost["card"], source, bindings)?;
                if cost.get("where").is_none() && required.is_none() {
                    return Err(EngineError::new(
                        "discard cost requires a supported filter or object reference",
                    ));
                }
                if !cost["where"].is_null() && !card_filter_supported(&cost["where"]) {
                    return Err(EngineError::new("unsupported discard cost filter"));
                }
                Ok(self.state.players[player_index]
                    .hand
                    .iter()
                    .filter(|card| {
                        required.as_ref().is_none_or(|id| id == &card.instance_id)
                            && (cost["where"].is_null()
                                || self.card_matches_target_filter(card, &cost["where"], decisions))
                    })
                    .map(|card| {
                        card_cost_option(
                            source,
                            player_id,
                            &format!("discard:{}", card.instance_id),
                            format!("Discard {}", card.definition.name),
                            "costDiscardCard",
                            json!(card.instance_id),
                        )
                    })
                    .collect())
            }
            Some("exileCard") => {
                let zone = cost["zone"].as_str().unwrap_or_default();
                let cards = match zone {
                    "graveyard" => &self.state.players[player_index].graveyard,
                    "hand" => &self.state.players[player_index].hand,
                    _ => return Err(EngineError::new("unsupported card exile cost zone")),
                };
                if !cost["where"].is_null() && !card_filter_supported(&cost["where"]) {
                    return Err(EngineError::new("unsupported card exile cost filter"));
                }
                Ok(cards
                    .iter()
                    .filter(|card| {
                        // `another … card`: the source card is excluded.
                        !(cost["excludeSource"] == true
                            && card.instance_id == source.card.instance_id)
                            && (cost["where"].is_null()
                                || self.card_matches_target_filter(card, &cost["where"], decisions))
                    })
                    .map(|card| {
                        card_cost_option(
                            source,
                            player_id,
                            &format!("exile:{}", card.instance_id),
                            format!("Exile {}", card.definition.name),
                            "costExileCard",
                            json!(card.instance_id),
                        )
                    })
                    .collect())
            }
            // Other decoded cost domains prepare their alternatives in the caller.
            _ => Ok(Vec::new()),
        }
    }

    /// The caller selects exclusively from the prepared canonical options.
    /// There is no mutable-engine callback between preparation and commitment;
    /// filters and numeric expressions remain the captured readonly choice.
    /// Revalidate the physical resource and payer immediately before mutation.
    pub(in crate::engine) fn commit_card_cost_payment<P: DecisionProvider>(
        &mut self,
        cost: &Value,
        payment: &LegalAction,
        source: &StackObject,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        if !matches!(
            value_kind(cost),
            Some(
                "payLife"
                    | "sacrificePermanent"
                    | "discardCard"
                    | "exileCard"
                    | "returnPermanentToOwnersHand"
            )
        ) {
            return Ok(false);
        }
        if payment.kind != ActionKind::ChooseResolution
            || payment.card_instance_id.as_deref() != Some(source.card.instance_id.as_str())
            || !payment.targets.is_empty()
            || !payment.target_order.is_empty()
        {
            return Err(EngineError::new("invalid prepared card cost payment"));
        }
        let player_id = payment.player_id.as_str();
        self.player_index(player_id)?;
        match value_kind(cost) {
            Some("payLife") => {
                let amount = payment.decisions["costLifePayment"]
                    .as_i64()
                    .and_then(|amount| i32::try_from(amount).ok())
                    .filter(|amount| *amount >= 0)
                    .ok_or_else(|| EngineError::new("invalid prepared life payment amount"))?;
                if !self.player_can_pay_life_cost(player_id, amount) {
                    return Err(EngineError::new(
                        "prepared life payment is no longer payable",
                    ));
                }
                self.pay_life_as_cost(player_id, &source.card.instance_id, amount)?;
            }
            Some("sacrificePermanent") => {
                let id = payment.decisions["costSacrificePermanent"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("prepared sacrifice has no object identity"))?;
                let (owner, index) = self
                    .permanent_position(id)
                    .ok_or_else(|| EngineError::new("prepared sacrifice left the battlefield"))?;
                let permanent = &self.state.players[owner].battlefield[index];
                if permanent.controller != player_id
                    || permanent.flags.get("phasedOut").copied().unwrap_or(false)
                    || !self.permanent_can_be_sacrificed(permanent)
                {
                    return Err(EngineError::new("prepared sacrifice is no longer payable"));
                }
                self.sacrifice_permanent_as_cost(player_id, &source.card.instance_id, id)?;
            }
            Some("discardCard") => {
                let id = payment.decisions["costDiscardCard"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("prepared discard has no object identity"))?;
                let payer = self.player_index(player_id)?;
                if !self.state.players[payer]
                    .hand
                    .iter()
                    .any(|card| card.instance_id == id)
                {
                    return Err(EngineError::new(
                        "prepared discard is no longer in payer hand",
                    ));
                }
                self.discard_card_as_cost(player_id, id)?;
            }
            Some("exileCard") => {
                let id = payment.decisions["costExileCard"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("prepared exile has no card identity"))?;
                let zone = cost["zone"].as_str().unwrap_or_default();
                if !self.move_card_to_exile(id, zone, player_id, "cost")? {
                    return Err(EngineError::new(
                        "prepared exile card is no longer in the payer's zone",
                    ));
                }
            }
            Some("returnPermanentToOwnersHand") => {
                let id = payment.decisions["costReturnPermanent"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("prepared return has no object identity"))?;
                let (owner, index) = self
                    .permanent_position(id)
                    .ok_or_else(|| EngineError::new("prepared return left the battlefield"))?;
                let permanent = &self.state.players[owner].battlefield[index];
                if !self.permanent_can_pay_return_cost(permanent, player_id, cost, source)
                    || (!cost["where"].is_null()
                        && !self.permanent_matches_effective_filter(permanent, &cost["where"]))
                {
                    return Err(EngineError::new("prepared return is no longer payable"));
                }
                let id = id.to_string();
                self.return_permanent_to_hand(&id)?;
            }
            _ => unreachable!("card cost domain was validated before commitment"),
        }
        Ok(true)
    }
}

impl GameEngine {
    /// A permanent the payer controls that a return cost may move: not
    /// phased out, not the excluded source, and not kept on the battlefield
    /// by a static prohibition of the same activation return cost.
    fn permanent_can_pay_return_cost(
        &self,
        permanent: &CardInstance,
        player_id: &str,
        cost: &Value,
        source: &StackObject,
    ) -> bool {
        permanent.controller == player_id
            && !permanent.flags.get("phasedOut").copied().unwrap_or(false)
            && !(cost["excludeSource"] == true && permanent.instance_id == source.card.instance_id)
            && !self.static_modifier_applies_to("cantBeBeamedUp", permanent)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "card_costs_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "return_cost_tests.rs"]
mod return_cost_tests;
