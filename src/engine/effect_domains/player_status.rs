//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("grantExtraTurn") => player_reference_supported(&effect["player"]),
        Some("grantNoMaximumHandSize") => player_reference_supported(&effect["player"]),
        Some("loseGame") => player_reference_supported(&effect["player"]),

        Some("ventureDungeon") => {
            player_reference_supported(&effect["player"])
                && effect.get("procedure").is_none_or(Value::is_string)
        }
        Some("grantPlayerKeyword") => {
            player_reference_supported(&effect["player"])
                && matches!(effect["keyword"].as_str(), Some("hexproof" | "shroud"))
                && match value_kind(&effect["duration"]) {
                    Some("untilEndOfCurrentTurn") => true,
                    Some("untilNextTurn") => {
                        player_reference_supported(&effect["duration"]["player"])
                    }
                    _ => false,
                }
        }
        Some("restrictPlayerActions") => {
            player_reference_supported(&effect["player"])
                && effect["castSpells"].is_boolean()
                && effect["activateAbilities"].is_boolean()
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        _ => false,
    }
}

/// The rules-state kind of a keyword granted to a player by a one-shot
/// effect (`you gain hexproof until end of turn`). `keyword` is the player
/// keyword kind the static modifiers use (`playerHexproof`, `playerShroud`).
pub(in crate::engine) const GRANTED_PLAYER_KEYWORD: &str = "grantedPlayerKeyword";

impl GameEngine {
    /// Whether a one-shot effect granted the player keyword `kind`
    /// (`playerHexproof`, `playerShroud`) and its duration has not ended.
    /// An `until end of turn` grant ends with its turn; an `until your next
    /// turn` grant is removed when that turn begins.
    pub(in crate::engine) fn player_has_granted_keyword(
        &self,
        player_id: &str,
        kind: &str,
    ) -> bool {
        self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some(GRANTED_PLAYER_KEYWORD)
                && modifier["playerId"].as_str() == Some(player_id)
                && modifier["keyword"].as_str() == Some(kind)
                && modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_none_or(|turn| turn >= u64::from(self.state.turn_number))
        })
    }

    pub(super) fn execute_player_status_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        _decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_player_status_domain_known(effect, stack_object, bindings, provider)
            .map(|()| true)
    }

    fn execute_player_status_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("grantNoMaximumHandSize") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "noMaximumHandSize",
                    "playerId": player_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "noMaximumHandSizeGranted",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("loseGame") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                    && self
                        .state
                        .players
                        .iter()
                        .any(|player| player.id == player_id && !player.has_lost)
                {
                    self.record_event(
                        "playerLostGame",
                        Some(player_id.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "reason": "effect" }),
                    );
                    self.eliminate_players(GameEndReason::LifeTotal, vec![player_id]);
                }
            }

            Some("grantExtraTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "extraTurnForPlayer",
                        "playerId": player_id,
                        "afterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("ventureDungeon") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    // CR 701.49c: a named procedure enters only its Dungeon.
                    let procedure = effect["procedure"].as_str();
                    self.venture_into_dungeon(stack_object, provider, &player_id, procedure)?;
                }
            }
            // CR 702.11d, 702.18d: the player has the keyword for the
            // stated duration; targeting reads it through the player's
            // keyword query (`player_has_granted_keyword`).
            Some("grantPlayerKeyword") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let keyword = match effect["keyword"].as_str() {
                    Some("hexproof") => "playerHexproof",
                    Some("shroud") => "playerShroud",
                    _ => {
                        return Err(EngineError::new(
                            "a granted player keyword requires hexproof or shroud",
                        ));
                    }
                };
                let mut modifier = json!({
                    "kind": GRANTED_PLAYER_KEYWORD,
                    "playerId": player_id,
                    "keyword": keyword,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                });
                if value_kind(&effect["duration"]) == Some("untilNextTurn") {
                    let Some(expiry_player_id) =
                        self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                    else {
                        return Err(EngineError::new(
                            "a player keyword until a next turn requires that turn's player",
                        ));
                    };
                    modifier["expiresAtTurnStart"] = Value::String(expiry_player_id);
                } else {
                    modifier["expiresAfterTurn"] = Value::from(self.state.turn_number);
                }
                self.state.rule_modifiers.push(modifier);
            }
            Some("restrictPlayerActions") => {
                let player_ids = if value_kind(&effect["player"]) == Some("eachPlayer") {
                    self.state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>()
                } else {
                    self.resolve_player(&effect["player"], stack_object, bindings)
                        .into_iter()
                        .collect()
                };
                for player_id in player_ids {
                    self.state.rule_modifiers.push(json!({
                        "kind": "playerActionRestriction",
                        "playerId": player_id,
                        "castSpells": effect["castSpells"],
                        "activateAbilities": effect["activateAbilities"],
                        "expiresAfterTurn": self.state.turn_number,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                    }));
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
