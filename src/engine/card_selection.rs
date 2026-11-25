//! Ordinary collection selection: query, predicate and cardinality are parameters.
//! Cross-selection historical flags/candidate-zone variants retain their caller.
use super::*;

/// One leaf of a card-zone query filter (`cardsInZone.where`): a definition
/// atom, or a characteristic a resolution choice of the same program records
/// (`with that name`, `of the chosen color`). Execution materializes such a
/// reference into its definition atom (`nameEquals`, `colorContains`) before
/// any candidate is read (CR 608.2d); a reference whose choice has not been
/// made stays unmaterialized and the definition-only card domain refuses it
/// at resolution (`runtime_collections`), never reading it as false.
pub(super) fn card_zone_query_atom_supported(leaf: &Value) -> bool {
    object_predicate::definition_atom_supported(leaf)
        || (matches!(value_kind(leaf), Some("chosenCardName" | "chosenColor"))
            && leaf["decisionId"].is_string())
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

pub(super) fn ordinary_bound_selection(effect: &Value) -> bool {
    value_kind(&effect["candidates"]) != Some("cards")
        && !effect["sameLandType"].as_bool().unwrap_or(false)
        && !effect["onePerControlledColor"].as_bool().unwrap_or(false)
        && value_kind(&effect["selection"]) != Some("random")
        && effect.get("extremum").is_none()
        && effect.get("setConstraints").is_none()
}

impl GameEngine {
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
