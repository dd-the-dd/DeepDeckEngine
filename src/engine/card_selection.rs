//! Ordinary collection selection: query, predicate and cardinality are parameters.
//! Cross-selection historical flags/candidate-zone variants retain their caller.
use super::*;

/// One leaf of a card-zone query filter (`cardsInZone.where`): a definition
/// atom, or a characteristic a resolution choice of the same program records
/// (`with that name`, `of the chosen color`). Execution materializes such a
/// reference into its definition atom (`nameEquals`, `colorContains`) before
/// any candidate is read (CR 608.2d); a reference whose choice has not been
/// made stays unmaterialized and the definition-only card domain refuses it
/// at resolution (`runtime_collections`), never reading it as false. A
/// card zone-history status (`put there from the battlefield this turn`) is
/// bound per candidate card by `object_event_history`.
pub(super) fn card_zone_query_atom_supported(leaf: &Value) -> bool {
    object_predicate::definition_atom_supported(leaf)
        || value_kind(leaf).is_some_and(object_event_history::is_card_history_kind)
        || (matches!(value_kind(leaf), Some("chosenCardName" | "chosenColor"))
            && leaf["decisionId"].is_string())
}

/// Declared card targets in the zone of the player a trigger's event
/// recorded (`target card from that player's graveyard`, CR 603.2): the
/// zone owner is resolved once from the trigger's event, as the controller
/// relation of permanent candidates is, so the candidate enumeration never
/// falls back to the ability controller's zone. `None` when the event
/// recorded no player: the trigger has no legal declaration.
pub(super) fn resolve_triggering_zone_owner(
    candidates: &mut Value,
    trigger_decisions: &BTreeMap<String, Value>,
) -> Option<()> {
    if value_kind(candidates) != Some("cards")
        || value_kind(&candidates["zone"]["player"]) != Some("triggeringPlayer")
    {
        return Some(());
    }
    let player = trigger_decisions
        .get("triggeringPlayerId")
        .and_then(Value::as_str)?;
    candidates["resolvedZonePlayerId"] = Value::String(player.to_string());
    Some(())
}

/// `libraryTopCards`: the top cards of one player's library (`player`) or
/// of each library of a player population (`players`: `the top card of
/// each player's library`, CR 401.2), with a supported count.
pub(super) fn library_top_cards_supported(expression: &Value) -> bool {
    let zone = &expression["zone"];
    value_kind(zone) == Some("library")
        && match (zone.get("player"), zone.get("players")) {
            (Some(player), None) => player_reference_supported(player),
            (None, Some(population)) => player_query::population_supported(population),
            _ => false,
        }
        && numeric_expression_supported(&expression["count"])
}

/// The cards a flashback grant names: one declared or bound card, or a card
/// collection (`each instant and sorcery card in your graveyard`) whose
/// members are fixed when the grant resolves (CR 611.2c).
pub(super) fn flashback_grant_objects_supported(object: &Value) -> bool {
    target_reference_supported(object) || object_collection_expression_supported(object)
}

/// The cardinality of a resolution selection: a stated `count`, or a range
/// the choosing player picks a member of (`minimum` and an optional
/// `maximum`; `any number of` has no maximum, CR 608.2d). Exactly one form.
pub(super) fn selection_cardinality_supported(effect: &Value) -> bool {
    match (effect.get("count"), effect.get("minimum")) {
        (Some(count), None) => {
            numeric_expression_supported(count) && effect.get("maximum").is_none()
        }
        (None, Some(minimum)) => {
            integer_value(minimum).is_some_and(|minimum| minimum >= 0)
                && effect
                    .get("maximum")
                    .is_none_or(numeric_expression_supported)
        }
        _ => false,
    }
}

/// The fewest members a selection takes, when it is a literal: a stated
/// literal count or a range's minimum. An optional instruction is available
/// only when that many candidates exist (CR 608.2d).
pub(super) fn literal_selection_minimum(effect: &Value) -> Option<usize> {
    integer_value(effect.get("minimum").unwrap_or(&effect["count"]))
        .and_then(|count| usize::try_from(count.max(0)).ok())
}

/// Bounded previews of every selection of `minimum..=maximum` members.
/// They are previews, not the legal domain: a direct response may pick any
/// subset of the candidates within the bounds.
pub(super) fn bounded_selections(
    candidates: &[String],
    minimum: usize,
    maximum: usize,
) -> Vec<Vec<String>> {
    let mut previews = Vec::new();
    for count in minimum..=maximum {
        if previews.len() >= 128 {
            break;
        }
        previews.extend(GameEngine::resolution_selections(
            candidates,
            count,
            128 - previews.len(),
        ));
    }
    previews
}

pub(super) fn ordinary_bound_selection(effect: &Value) -> bool {
    value_kind(&effect["candidates"]) != Some("cards")
        && !effect["sameLandType"].as_bool().unwrap_or(false)
        && !effect["onePerControlledColor"].as_bool().unwrap_or(false)
        && value_kind(&effect["selection"]) != Some("random")
        && effect.get("extremum").is_none()
        && effect.get("setConstraints").is_none()
}

impl GameEngine {
    /// The bounds of a selection's cardinality (`selection_cardinality_supported`)
    /// under the resolving object: `(minimum, maximum)`, where `None` leaves
    /// only the candidates to bound the choice. A stated count is both.
    pub(super) fn selection_cardinality(
        &self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        field: &'static str,
    ) -> Result<(usize, Option<usize>), EngineError> {
        let amount = |expression: &Value| {
            self.checked_runtime_numeric_expression(
                expression,
                stack_object,
                bindings,
                decisions,
                field,
            )
            .map(|amount| amount.max(0) as usize)
        };
        let Some(minimum) = effect.get("minimum") else {
            let count = amount(&effect["count"])?;
            return Ok((count, Some(count)));
        };
        let minimum = amount(minimum)?;
        let maximum = effect.get("maximum").map(amount).transpose()?;
        if maximum.is_some_and(|maximum| maximum < minimum) {
            return Err(EngineError::new(format!(
                "{field}: the selection minimum exceeds its maximum"
            )));
        }
        Ok((minimum, maximum))
    }

    /// The top `count` cards of each library the expression names, each
    /// library top first (CR 401.2), libraries in the population's player
    /// order. The set is identified when evaluated; a `bind` fixes it for
    /// later verbs (CR 608.2c). An unresolved holder or count yields `None`.
    pub(super) fn library_top_card_ids(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Vec<String>> {
        let zone = &expression["zone"];
        let holders = match zone.get("players") {
            Some(population) => player_query::PlayerQueryContext::runtime(stack_object, bindings)
                .resolve_population(self, population)
                .ok()?,
            None => vec![self.resolve_player(&zone["player"], stack_object, bindings)?],
        };
        let count = self
            .checked_runtime_numeric_expression(
                &expression["count"],
                stack_object,
                bindings,
                decisions,
                "libraryTopCards.count",
            )
            .ok()?
            .max(0) as usize;
        let mut cards = Vec::new();
        for holder in holders {
            let player_index = self.player_index(&holder).ok()?;
            cards.extend(
                self.state.players[player_index]
                    .library
                    .iter()
                    .rev()
                    .take(count)
                    .map(|card| card.instance_id.clone()),
            );
        }
        Some(cards)
    }

    /// `CARDS gain flashback until end of turn` (CR 702.34a): each card
    /// outside the battlefield the object names when the grant resolves may
    /// be cast this turn by the grant's controller for its mana cost, and is
    /// exiled when it leaves the stack. A declared card that is no longer
    /// there gains nothing (CR 400.7).
    pub(super) fn grant_flashback_until_end_of_turn(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let object = &effect["object"];
        let instance_ids = if target_reference_supported(object) {
            match self.resolve_target(object, stack_object, bindings) {
                Some(TargetRef::Card { instance_id }) => vec![instance_id],
                _ => Vec::new(),
            }
        } else {
            self.resolve_runtime_object_ids_checked(object, stack_object, bindings, decisions)?
        };
        for instance_id in instance_ids {
            let Some(card) = self.card_outside_battlefield(&instance_id).cloned() else {
                continue;
            };
            let permission = json!({
                "action": "cast",
                "alternativeManaCost": card.definition.mana_cost,
                "cardInstanceId": instance_id,
                "exileOnLeaveStack": true,
                "expiresAfterTurn": self.state.turn_number,
                "normalCostsApply": true,
                "normalTimingApplies": true,
                // CR 702.34a: the card's own flashback is cast by the
                // player it lets cast the card, its owner, whose zone it is.
                "playerId": card.owner,
                "sourceCardInstanceId": stack_object.card.instance_id,
            });
            self.state.permissions.push(permission.clone());
            self.record_event(
                "flashbackPermissionGranted",
                Some(card.owner.clone()),
                Some(card.instance_id),
                permission,
            );
        }
        Ok(())
    }

    pub(super) fn execute_bound_card_selection<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let id = effect["id"]
            .as_str()
            .ok_or_else(|| EngineError::new("chooseCards requires a decision identifier"))?;
        let player = self
            .resolve_player(&effect["player"], source, bindings)
            .ok_or_else(|| EngineError::new("unresolved chooseCards player reference"))?;
        self.player_index(&player)?;
        // Pure independent operands are prepared even when the source is empty.
        let predicate = effect
            .get("where")
            .map(|predicate| {
                self.prepare_runtime_object_filter(predicate, source, bindings, decisions)
            })
            .transpose()?;
        let mut objects =
            self.resolve_runtime_object_ids_checked(&effect["from"], source, bindings, decisions)?;
        if let Some(predicate) = predicate {
            objects.retain(|id| {
                self.runtime_collection_object_matches(id, &predicate, source, bindings, decisions)
            });
        }
        if objects.iter().collect::<BTreeSet<_>>().len() != objects.len() {
            return Err(EngineError::new(
                "chooseCards collection contains duplicate object identities",
            ));
        }
        let quantity = |expression: &Value, field: &'static str| {
            self.checked_runtime_numeric_expression(expression, source, bindings, decisions, field)
                .map(|n| n.max(0) as usize)
        };
        let (minimum, maximum) =
            if effect.get("minimum").is_some() || effect.get("maximum").is_some() {
                (
                    quantity(&effect["minimum"], "chooseCards.minimum")?,
                    quantity(&effect["maximum"], "chooseCards.maximum")?,
                )
            } else {
                let count = quantity(&effect["count"], "chooseCards.count")?;
                (count, count)
            };
        if minimum > maximum {
            return Err(EngineError::new("chooseCards minimum exceeds maximum"));
        }
        let maximum = maximum.min(objects.len());
        let minimum = minimum.min(maximum);
        // These are bounded fallback strategy previews, not the legal domain.
        // With no cross-selection constraints their length is1 iff the full
        // domain is singleton. Direct structured responses may select ANY
        // subset satisfying the full candidate IDs/cardinality contract.
        let mut previews = Vec::new();
        for count in minimum..=maximum {
            previews.extend(Self::resolution_selections(
                &objects,
                count,
                128 - previews.len(),
            ));
            if previews.len() >= 128 {
                break;
            }
        }
        self.make_resolution_decision(
            id,
            &player,
            source,
            DecisionChoice::CardSelection {
                decision_id: id.to_owned(),
                candidate_card_instance_ids: objects,
                minimum,
                maximum,
                prompt: Self::resolution_choice_prompt(id, next_effect, false),
            },
            previews,
            decisions,
            provider,
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "card_selection_tests.rs"]
mod tests;
