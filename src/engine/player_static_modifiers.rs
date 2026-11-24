//! Battlefield-active static modifiers whose subject is a set of players
//! (CR 604.1, 611.3): player keywords, cast and activation prohibitions,
//! hand size, draw and untap limits, and other players' untap steps.
//!
//! Each modifier names its player population relative to its source
//! (`you`, `each opponent`, `each player`, an attached permanent's
//! controller). The population is evaluated continuously, so a change of
//! control of the source changes the affected players.
use super::*;

/// Player populations a static player modifier may name, relative to its
/// source.
pub(super) fn static_player_population_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "eachPlayer") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("controllerOfAttachedPermanent") => {
            value_kind(&reference["attachment"]) == Some("self")
        }
        Some("opponentsOf") => static_player_population_supported(&reference["player"]),
        _ => false,
    }
}

fn positive_integer(value: &Value) -> bool {
    integer_value(value).is_some_and(|value| value > 0)
}

/// Validation of the player-subject static modifiers this module owns.
/// `None` leaves the modifier to the battlefield modifier validator.
pub(super) fn modifier_supported(modifier: &Value) -> Option<bool> {
    let population = |field: &str| static_player_population_supported(&modifier[field]);
    match value_kind(modifier) {
        // CR 101.2: `PLAYERS can't cast SPELLS [during combat | during your turn]`.
        Some("prohibitCast") => Some(
            population("players")
                && modifier.get("where").is_none_or(card_filter_supported)
                && modifier
                    .get("duringTurnOf")
                    .is_none_or(|turn| singular_player_reference_supported(turn))
                && modifier
                    .get("duringCombat")
                    .is_none_or(|combat| combat.as_bool() == Some(true)),
        ),
        // CR 402.2: a signed adjustment of a maximum hand size.
        Some("modifyMaximumHandSize") => {
            Some(population("player") && integer_value(&modifier["amount"]).is_some_and(|a| a != 0))
        }
        // CR 121.2: `can't draw more than N cards each turn`.
        Some("drawLimitPerTurn") => {
            Some(population("players") && positive_integer(&modifier["maximum"]))
        }
        // CR 502.3: `can't untap more than N OBJECTS during their untap steps`.
        Some("untapLimitPerUntapStep") => Some(
            population("players")
                && value_kind(&modifier["objects"]) == Some("permanents")
                && static_object_selector_supported(&modifier["objects"])
                && positive_integer(&modifier["maximum"]),
        ),
        // CR 602.5: an activation prohibition scoped by an object selector
        // and, optionally, by the players it binds.
        Some("prohibitActivation")
            if modifier.get("players").is_some()
                || modifier["abilities"].get("sources").is_some() =>
        {
            Some(
                modifier
                    .get("players")
                    .is_none_or(static_player_population_supported)
                    && value_kind(&modifier["abilities"]) == Some("activatedAbilities")
                    && static_object_selector_supported(&modifier["abilities"]["sources"])
                    && (modifier["abilities"]["where"].is_null()
                        || (value_kind(&modifier["abilities"]["where"]) == Some("not")
                            && value_kind(&modifier["abilities"]["where"]["operand"])
                                == Some("isManaAbility"))),
            )
        }
        // CR 502.3: permanents that also untap during other players' untap steps.
        Some("untapDuringOtherPlayersUntap") if modifier.get("objects").is_some() => {
            Some(static_object_selector_supported(&modifier["objects"]))
        }
        _ => None,
    }
}

impl GameEngine {
    /// Whether a source-relative player population includes `player_id`. A
    /// modifier without a population this module recognizes keeps the
    /// historical meaning: its source's controller.
    pub(super) fn static_population_includes(
        &self,
        source: &CardInstance,
        reference: &Value,
        player_id: &str,
    ) -> bool {
        if static_player_population_supported(reference) {
            self.event_player_reference_matches(reference, source, player_id)
        } else {
            source.controller == player_id
        }
    }

    /// Whether an active static modifier of `kind` applies to `player_id`
    /// through its `player` population.
    pub(super) fn player_static_modifier_present(&self, player_id: &str, kind: &str) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                value_kind(modifier) == Some(kind)
                    && self.static_population_includes(source, &modifier["player"], player_id)
            })
    }

    /// CR 402.2: the maximum hand size after static adjustments. No maximum
    /// stays unbounded.
    pub(super) fn static_adjusted_maximum_hand_size(
        &self,
        player_id: &str,
        maximum: usize,
    ) -> usize {
        if maximum == usize::MAX {
            return maximum;
        }
        let adjustment = self
            .battlefield_static_modifiers()
            .iter()
            .filter(|(source, modifier)| {
                value_kind(modifier) == Some("modifyMaximumHandSize")
                    && self.static_population_includes(source, &modifier["player"], player_id)
            })
            .filter_map(|(_, modifier)| integer_value(&modifier["amount"]))
            .map(i64::from)
            .sum::<i64>();
        let adjusted = i64::try_from(maximum)
            .unwrap_or(i64::MAX)
            .saturating_add(adjustment)
            .max(0);
        usize::try_from(adjusted).unwrap_or(0)
    }

    /// CR 101.2: whether a static cast prohibition applies to `player_id`
    /// now. Without a card, only prohibitions independent of the spell's
    /// characteristics are decided; a spell filter needs the card.
    pub(super) fn static_cast_prohibition_applies(
        &self,
        source: &CardInstance,
        modifier: &Value,
        player_id: &str,
        card: Option<&CardDefinition>,
    ) -> bool {
        if value_kind(modifier) != Some("prohibitCast") {
            return false;
        }
        let active_player_id = &self.state.players[self.state.active_player].id;
        let in_combat = matches!(
            self.state.step,
            GameStep::BeginningOfCombat
                | GameStep::DeclareAttackers
                | GameStep::DeclareBlockers
                | GameStep::CombatDamage
        );
        self.static_population_includes(source, &modifier["players"], player_id)
            && modifier.get("duringTurnOf").is_none_or(|turn| {
                self.event_player_reference_matches(turn, source, active_player_id)
            })
            && (modifier["duringCombat"].as_bool() != Some(true) || in_combat)
            && match (modifier.get("where"), card) {
                (None, _) => true,
                (Some(filter), Some(card)) => matches_card_filter(card, filter),
                (Some(_), None) => false,
            }
    }

    /// CR 101.2: a spell-filtered static cast prohibition for this card.
    pub(super) fn static_card_cast_prohibited(
        &self,
        player_id: &str,
        card: &CardDefinition,
    ) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                modifier.get("where").is_some()
                    && self.static_cast_prohibition_applies(source, modifier, player_id, Some(card))
            })
    }

    /// CR 121.2: a static limit on the cards a player can draw each turn has
    /// been reached; a further draw doesn't happen.
    pub(super) fn static_draw_limit_reached(&self, player_id: &str) -> bool {
        let drawn = self
            .state
            .events
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == "cardDrawn"
                    && event.player_id.as_deref() == Some(player_id)
            })
            .count();
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                value_kind(modifier) == Some("drawLimitPerTurn")
                    && self.static_population_includes(source, &modifier["players"], player_id)
                    && integer_value(&modifier["maximum"])
                        .and_then(|maximum| usize::try_from(maximum).ok())
                        .is_some_and(|maximum| drawn >= maximum)
            })
    }

    /// CR 602.5: the scope of a static activation prohibition: its sources
    /// (an object selector, else a printed card filter), the players it binds
    /// (the controller who would activate), and its rule's condition.
    pub(super) fn static_activation_prohibition_scope_matches(
        &self,
        source: &CardInstance,
        rule: &Value,
        modifier: &Value,
        permanent: &CardInstance,
    ) -> bool {
        let abilities = &modifier["abilities"];
        let sources_match = if abilities.get("sources").is_some() {
            Self::static_selector_matches(source, &abilities["sources"], permanent)
        } else {
            self.activation_source_filter_matches(source, permanent, &abilities["sourceWhere"])
        };
        sources_match
            && modifier.get("players").is_none_or(|players| {
                self.static_population_includes(source, players, &permanent.controller)
            })
            && rule.get("condition").is_none_or(|condition| {
                self.condition_value_with_source(
                    condition,
                    &source.controller,
                    &BTreeMap::new(),
                    Some(source),
                )
            })
    }

    /// CR 502.3: permanents that untap during the active player's untap step
    /// although another player controls them.
    pub(super) fn static_other_player_untap_ids(&self, active_player_id: &str) -> Vec<String> {
        self.battlefield_static_modifiers()
            .into_iter()
            .filter(|(source, modifier)| {
                source.controller != active_player_id
                    && value_kind(modifier) == Some("untapDuringOtherPlayersUntap")
            })
            .flat_map(|(source, modifier)| {
                if modifier.get("objects").is_some() {
                    self.state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            permanent.tapped
                                && Self::static_selector_matches(
                                    &source,
                                    &modifier["objects"],
                                    permanent,
                                )
                        })
                        .map(|permanent| permanent.instance_id.clone())
                        .collect::<Vec<_>>()
                } else if source.tapped {
                    vec![source.instance_id.clone()]
                } else {
                    Vec::new()
                }
            })
            .collect()
    }

    /// CR 502.3 with a static untap limit: of the tapped permanents a limit
    /// covers, the active player chooses which ones untap, up to the limit.
    pub(super) fn apply_static_untap_limits<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
        active_player_id: &str,
        candidates: Vec<String>,
    ) -> Result<Vec<String>, EngineError> {
        let limits = self
            .battlefield_static_modifiers()
            .into_iter()
            .filter(|(source, modifier)| {
                value_kind(modifier) == Some("untapLimitPerUntapStep")
                    && self.static_population_includes(
                        source,
                        &modifier["players"],
                        active_player_id,
                    )
            })
            .collect::<Vec<_>>();
        let mut candidates = candidates;
        for (limit_index, (source, modifier)) in limits.iter().enumerate() {
            let Some(maximum) = integer_value(&modifier["maximum"])
                .and_then(|maximum| usize::try_from(maximum).ok())
            else {
                continue;
            };
            let covered = candidates
                .iter()
                .filter(|instance_id| {
                    self.permanent_position(instance_id).is_some_and(
                        |(player_index, card_index)| {
                            let permanent =
                                &self.state.players[player_index].battlefield[card_index];
                            permanent.tapped
                                && Self::static_selector_matches(
                                    source,
                                    &modifier["objects"],
                                    permanent,
                                )
                        },
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            if covered.len() <= maximum {
                continue;
            }
            let mut chosen = Vec::<String>::new();
            while chosen.len() < maximum {
                let options = covered
                    .iter()
                    .filter(|instance_id| !chosen.contains(instance_id))
                    .map(|instance_id| {
                        let name = self
                            .permanent_position(instance_id)
                            .map(|(player_index, card_index)| {
                                self.state.players[player_index].battlefield[card_index]
                                    .definition
                                    .name
                                    .clone()
                            })
                            .unwrap_or_default();
                        LegalAction {
                            id: format!("untapWithinLimit:{instance_id}"),
                            kind: ActionKind::ChooseResolution,
                            player_id: active_player_id.to_string(),
                            label: format!("Untap {name}"),
                            card_instance_id: Some(instance_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::new(),
                            targets: BTreeMap::new(),
                            target_order: Vec::new(),
                            attacker_id: None,
                            blocker_id: None,
                        }
                    })
                    .collect::<Vec<_>>();
                let choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!(
                            "untapLimit:{}:{limit_index}:{}",
                            self.state.turn_number,
                            chosen.len()
                        ),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: active_player_id.to_string(),
                        source_card: Some(source.clone()),
                        source_card_instance_id: Some(source.instance_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                let Some(instance_id) = choice.card_instance_id else {
                    return Err(EngineError::new(
                        "an untap-limit choice must name one covered permanent",
                    ));
                };
                if !covered.contains(&instance_id) || chosen.contains(&instance_id) {
                    return Err(EngineError::new(
                        "an untap-limit choice named a permanent outside the limit",
                    ));
                }
                chosen.push(instance_id);
            }
            candidates.retain(|instance_id| {
                !covered.contains(instance_id) || chosen.contains(instance_id)
            });
        }
        Ok(candidates)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_static_modifiers_tests.rs"]
mod tests;
