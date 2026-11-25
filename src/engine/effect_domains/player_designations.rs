//! A player's rules state changed by a one-shot instruction: a player
//! designation (CR 724 the monarch, CR 725 the initiative), the Ring's temptation (CR 701.54) and a
//! skipped part of the player's future turns (CR 500.11).
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("becomeMonarch" | "temptWithRing") => player_reference_supported(&effect["player"]),
        Some("takeInitiative") => crate::engine::initiative::take_initiative_supported(effect),
        Some("skipTurnPart") => {
            player_reference_supported(&effect["player"])
                && matches!(
                    effect["part"].as_str(),
                    Some(
                        "turn"
                            | "untapStep"
                            | "drawStep"
                            | "combatPhase"
                            | "allCombatPhasesOfNextTurn"
                    )
                )
                && effect["count"].as_u64().is_some_and(|count| count > 0)
                && effect["thisTurn"].is_boolean()
                && (effect["part"].as_str() != Some("turn")
                    || effect["thisTurn"].as_bool() == Some(false))
        }
        _ => false,
    })
}

impl GameEngine {
    pub(super) fn execute_player_designations_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        _decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(true);
        };
        match value_kind(effect) {
            // CR 724.4: the player becomes the monarch; the previous
            // monarch, if any, stops being the monarch.
            Some("becomeMonarch") => self.become_monarch(&player_id),
            // CR 725.3-725.4: the initiative and its venture trigger.
            Some("takeInitiative") => self.take_initiative(&player_id)?,
            Some("temptWithRing") => {
                self.tempt_player_with_ring(&player_id, stack_object, provider)?;
            }
            Some("skipTurnPart") => {
                let part = effect["part"].as_str().unwrap_or("turn").to_string();
                self.schedule_skipped_turn_part(
                    &player_id,
                    &part,
                    effect["count"].as_u64().unwrap_or(1),
                    effect["thisTurn"].as_bool().unwrap_or(false),
                );
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(true)
    }

    /// CR 701.54a: the player chooses a creature they control as their
    /// Ring-bearer (they must choose one if they control a creature; they
    /// may keep the same one), then the Ring gains its next ability, up to
    /// four (CR 701.54c). `Whenever the Ring tempts you` triggers.
    fn tempt_player_with_ring<P: DecisionProvider>(
        &mut self,
        player_id: &str,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let player_index = self.player_index(player_id)?;
        let candidates = self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|card| card.controller == player_id && self.permanent_is_creature(card))
            .map(|card| card.instance_id.clone())
            .collect::<Vec<_>>();
        let bearer_id = if candidates.is_empty() {
            None
        } else {
            let values = Self::resolution_selections(&candidates, 1, 128);
            let mut local_decisions = BTreeMap::new();
            self.make_resolution_decision(
                "chooseRingBearer",
                player_id,
                stack_object,
                DecisionChoice::CardSelection {
                    decision_id: "chooseRingBearer".to_string(),
                    candidate_card_instance_ids: candidates,
                    minimum: 1,
                    maximum: 1,
                    prompt: "Choose your Ring-bearer".to_string(),
                },
                values,
                &mut local_decisions,
                provider,
            )?;
            local_decisions["chooseRingBearer"]
                .as_array()
                .and_then(|selected| selected.first())
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        };
        let existing = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some("ringEmblem")
                && modifier["playerId"].as_str() == Some(player_id)
        });
        let level = existing
            .and_then(|index| self.state.rule_modifiers[index]["level"].as_i64())
            .unwrap_or(0)
            .saturating_add(1)
            .min(4);
        let modifier = json!({
            "kind": "ringEmblem",
            "playerId": player_id,
            "level": level,
            "bearerId": bearer_id,
        });
        if let Some(index) = existing {
            self.state.rule_modifiers[index] = modifier;
        } else {
            self.state.rule_modifiers.push(modifier);
        }
        self.record_event(
            "ringTempted",
            Some(player_id.to_string()),
            bearer_id,
            json!({ "level": level }),
        );
        self.enqueue_ring_tempted_triggers(player_id);
        Ok(())
    }
}

const MAX_MEMBER_DEPTH: usize = 64;

/// A nested quantifier binds its own `quantifiedPlayer`: inside a member
/// predicate it would be read against the wrong player and is refused.
fn without_nested_quantifier(value: &Value, depth: usize) -> bool {
    if depth >= MAX_MEMBER_DEPTH
        || matches!(
            value_kind(value),
            Some("playerQuantifier" | "opponentExtremum" | "filterPlayers")
        )
    {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| without_nested_quantifier(value, depth + 1)),
        Value::Object(object) => object
            .values()
            .all(|value| without_nested_quantifier(value, depth + 1)),
        _ => true,
    }
}

/// A predicate or measure about the population member alone, written with
/// the `quantifiedPlayer` reference a `playerQuantifier` also binds
/// (CR 102.1-102.3).
fn names_only_the_population_member(value: &Value) -> bool {
    without_nested_quantifier(value, 0)
        && crate::engine::player_quantifier::names_only_the_quantified_player(value, 0)
}

/// The population a filter restricts: every player, or the players other
/// than one singular player (`opponentsOf`, `playersExcept`).
fn filtered_population_supported(players: &Value) -> bool {
    match value_kind(players) {
        Some("eachPlayer") => true,
        Some("opponentsOf" | "playersExcept") => {
            singular_player_reference_supported(&players["player"])
        }
        _ => false,
    }
}

/// `filterPlayers`: `{players, except?, where?, superlative?}` with at
/// least one restriction. `where` is one condition about the member and
/// `superlative.value` one measure of the member, both written with
/// `quantifiedPlayer`.
pub(in crate::engine) fn filtered_players_supported(reference: &Value) -> bool {
    let member_condition = |condition: &Value| {
        names_only_the_population_member(condition)
            && condition_supported(&crate::engine::player_quantifier::bind_quantified_player(
                condition,
            ))
    };
    let member_measure = |value: &Value| {
        names_only_the_population_member(value)
            && numeric_expression_supported(
                &crate::engine::player_quantifier::bind_quantified_player(value),
            )
    };
    value_kind(reference) == Some("filterPlayers")
        && filtered_population_supported(&reference["players"])
        && (reference.get("except").is_some()
            || reference.get("where").is_some()
            || reference.get("superlative").is_some())
        && reference
            .get("except")
            .is_none_or(singular_player_reference_supported)
        && reference.get("where").is_none_or(member_condition)
        && reference.get("superlative").is_none_or(|superlative| {
            matches!(superlative["extremum"].as_str(), Some("greatest" | "least"))
                && superlative.get("unique").is_none_or(Value::is_boolean)
                && value_kind(&reference["players"]) == Some("eachPlayer")
                && reference.get("except").is_none()
                && member_measure(&superlative["value"])
        })
}

/// `seatNeighbor`: the nearest living player to the left or right of one
/// singular player (CR 101.4).
pub(in crate::engine) fn seat_neighbor_supported(reference: &Value) -> bool {
    value_kind(reference) == Some("seatNeighbor")
        && matches!(reference["direction"].as_str(), Some("left" | "right"))
        && singular_player_reference_supported(&reference["player"])
}

impl GameEngine {
    /// The nearest player still in the game to the left (the next player in
    /// turn order, CR 101.4) or to the right of the anchor player. In a
    /// two-player game both neighbors are the opponent; a player alone at
    /// the table has no neighbor.
    pub(in crate::engine) fn seat_neighbor(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<String> {
        let anchor = self.resolve_player(&reference["player"], stack_object, bindings)?;
        let anchor_index = self.player_index(&anchor).ok()?;
        let count = self.state.players.len();
        let left = match reference["direction"].as_str()? {
            "left" => true,
            "right" => false,
            _ => return None,
        };
        (1..count)
            .map(|offset| {
                if left {
                    (anchor_index + offset) % count
                } else {
                    (anchor_index + count - offset) % count
                }
            })
            .find(|index| !self.state.players[*index].has_lost)
            .map(|index| self.state.players[index].id.clone())
    }

    /// The members of a `filterPlayers` population: not the excluded
    /// player, satisfying `where`, and, for a superlative, holding the
    /// extreme value among every living player (ties included). Each
    /// member is read as the reference player of the shared condition and
    /// numeric evaluators, as in `playerQuantifier`. An unresolved excluded
    /// player or measure yields no member.
    pub(in crate::engine) fn filtered_players(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Vec<String> {
        let members = self
            .resolve_players(&reference["players"], stack_object, bindings)
            .into_iter()
            .filter(|id| {
                self.player_index(id)
                    .is_ok_and(|index| !self.state.players[index].has_lost)
            })
            .collect::<Vec<_>>();
        self.restrict_population(reference, members, stack_object, bindings)
            .unwrap_or_default()
    }

    /// The `filterPlayers` population as an iteration: the base population
    /// in turn order (fail-closed like every player iteration), then the
    /// restriction. An unresolved excluded player is an error, never an
    /// empty or unrestricted population.
    pub(in crate::engine) fn filtered_player_iteration_ids(
        &mut self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Vec<String>, EngineError> {
        let members = self.player_iteration_ids(&reference["players"], stack_object, bindings)?;
        self.restrict_population(reference, members, stack_object, bindings)
            .ok_or_else(|| EngineError::new("filtered player population is unresolved"))
    }

    fn restrict_population(
        &self,
        reference: &Value,
        members: Vec<String>,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<Vec<String>> {
        let except = match reference.get("except") {
            Some(player) => Some(self.resolve_player(player, stack_object, bindings)?),
            None => None,
        };
        let condition = reference
            .get("where")
            .map(crate::engine::player_quantifier::bind_quantified_player);
        let source = Some(&stack_object.card);
        let mut members = members
            .into_iter()
            .filter(|member| except.as_deref() != Some(member.as_str()))
            .filter(|member| {
                condition.as_ref().is_none_or(|condition| {
                    self.condition_value_with_source(
                        condition,
                        member,
                        &stack_object.decisions,
                        source,
                    )
                })
            })
            .collect::<Vec<_>>();
        if let Some(superlative) = reference.get("superlative") {
            let measure =
                crate::engine::player_quantifier::bind_quantified_player(&superlative["value"]);
            let values = self
                .state
                .players
                .iter()
                .filter(|player| !player.has_lost)
                .map(|player| {
                    self.numeric_expression_with_source(&measure, &player.id, source)
                        .map(|value| (player.id.clone(), value))
                })
                .collect::<Option<Vec<_>>>()?;
            let extreme = match superlative["extremum"].as_str()? {
                "greatest" => values.iter().map(|(_, value)| *value).max()?,
                "least" => values.iter().map(|(_, value)| *value).min()?,
                _ => return None,
            };
            members.retain(|member| {
                values
                    .iter()
                    .any(|(id, value)| id == member && *value == extreme)
            });
            // `the player with the most life`: a tie singles out no player.
            let holders = values.iter().filter(|(_, value)| *value == extreme).count();
            if superlative["unique"].as_bool() == Some(true) && holders != 1 {
                members.clear();
            }
        }
        Some(members)
    }
}
