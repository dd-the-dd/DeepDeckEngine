//! Resolve the collection operand of a counter-placement primitive.
//! Predicate evaluation and player resolution remain shared engine services.

use super::{
    EngineError, GameEngine, RuntimeBinding, StackObject, object_expression_supported,
    permanent_filter_supported, singular_player_reference_supported, value_kind,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn player_scope_supported(player: &Value) -> bool {
    match value_kind(player) {
        Some("eachPlayer") => true,
        Some("opponentsOf") => singular_player_reference_supported(&player["player"]),
        _ => singular_player_reference_supported(player),
    }
}

pub(super) fn counter_collection_selector_supported(selector: &Value) -> bool {
    value_kind(selector) == Some("eachPermanent")
        && selector.get("where").is_none_or(|filter| {
            // The collection prepares a relation's anchor before matching.
            super::object_relation_eval::filter_supported_with_relations(
                filter,
                permanent_filter_supported,
            )
        })
        && selector.get("player").is_none_or(player_scope_supported)
        && ["excludeSource", "excludeSourceController"]
            .iter()
            .all(|key| selector.get(*key).is_none_or(Value::is_boolean))
        && selector
            .get("exclude")
            .is_none_or(object_expression_supported)
}

impl GameEngine {
    fn counter_collection_players(
        &self,
        scope: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<BTreeSet<String>, EngineError> {
        if !player_scope_supported(scope) {
            return Err(EngineError::new(
                "unsupported explicit counter collection scope",
            ));
        }
        // Validate a singular anchor before resolving a set. The common set
        // resolver deliberately returns an empty set for an unresolved anchor;
        // here that cannot silently turn into an unscoped collection.
        match value_kind(scope) {
            Some("eachPlayer") => {}
            Some("opponentsOf") => {
                let anchor = self
                    .resolve_player(&scope["player"], stack, bindings)
                    .ok_or_else(|| {
                        EngineError::new("explicit counter collection scope is unresolved")
                    })?;
                self.player_index(&anchor)?;
            }
            _ => {
                let player = self.resolve_player(scope, stack, bindings).ok_or_else(|| {
                    EngineError::new("explicit counter collection scope is unresolved")
                })?;
                self.player_index(&player)?;
            }
        }
        Ok(self
            .resolve_players(scope, stack, bindings)
            .into_iter()
            .collect())
    }

    pub(super) fn counter_collection_ids(
        &self,
        selector: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        excluded: &BTreeSet<String>,
    ) -> Result<Vec<String>, EngineError> {
        if !counter_collection_selector_supported(selector) {
            return Err(EngineError::new("unsupported counter collection selector"));
        }
        let controllers = selector
            .get("player")
            .map(|scope| self.counter_collection_players(scope, stack, bindings))
            .transpose()?;
        let predicate = selector
            .get("where")
            .map(|filter| self.prepare_relation_leaves(filter, stack, bindings))
            .transpose()?;
        Ok(self
            .state
            .players
            .iter()
            .flat_map(|player| &player.battlefield)
            .filter(|permanent| {
                !excluded.contains(&permanent.instance_id)
                    && (selector["excludeSource"].as_bool() != Some(true)
                        || permanent.instance_id != stack.card.instance_id)
                    && (selector["excludeSourceController"].as_bool() != Some(true)
                        || permanent.controller != stack.controller)
                    && controllers
                        .as_ref()
                        .is_none_or(|players| players.contains(&permanent.controller))
                    && predicate.as_ref().is_none_or(|predicate| {
                        self.permanent_matches_effective_filter(permanent, predicate)
                    })
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "counter_selection_tests.rs"]
mod tests;
