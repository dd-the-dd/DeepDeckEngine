//! Ordered graveyard positions (CR 404.1 as printed on older cards: a
//! graveyard keeps the order cards were put into it; the most recent card
//! is on top). `graveyardEndCards` names the top or bottom N cards of one
//! player's graveyard matching an optional card filter, counted from that
//! end among the matching cards only (`the top creature card of your
//! graveyard` is the most recent creature card, wherever it sits).
//!
//! The engine appends every card put into a graveyard, so the last card of
//! the zone vector is its top card; no other order is inferred.
use super::*;

/// `graveyardEndCards`: one player's graveyard, an end (`top` or
/// `bottom`), a supported count and an optional definition filter.
pub(super) fn graveyard_end_cards_supported(expression: &Value) -> bool {
    let zone = &expression["zone"];
    value_kind(zone) == Some("graveyard")
        && zone.get("players").is_none()
        && player_reference_supported(&zone["player"])
        && matches!(expression["end"].as_str(), Some("top" | "bottom"))
        && numeric_expression_supported(&expression["count"])
        && expression.get("where").is_none_or(|predicate| {
            card_filter_supported(predicate)
                && object_predicate::try_map_predicate_leaves(predicate, 0, &mut |leaf| {
                    if card_selection::card_zone_query_atom_supported(leaf) {
                        Ok(leaf.clone())
                    } else {
                        Err(EngineError::new("not a definition atom"))
                    }
                })
                .is_ok()
        })
}

/// The order a graveyard end reads its cards in: from that end inward.
pub(super) fn graveyard_end_order<'a>(
    graveyard: &'a [CardInstance],
    end: &str,
) -> Box<dyn Iterator<Item = &'a CardInstance> + 'a> {
    if end == "bottom" {
        Box::new(graveyard.iter())
    } else {
        Box::new(graveyard.iter().rev())
    }
}

impl GameEngine {
    /// The ids of the matching cards nearest the named end of the player's
    /// graveyard, from that end inward, at most `count` of them. `None` when
    /// the player or the count cannot be resolved (never an empty guess).
    pub(super) fn graveyard_end_card_ids(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Vec<String>> {
        let holder = self.resolve_player(&expression["zone"]["player"], stack_object, bindings)?;
        let end = expression["end"].as_str()?;
        let count = self
            .checked_runtime_numeric_expression(
                &expression["count"],
                stack_object,
                bindings,
                decisions,
                "graveyardEndCards.count",
            )
            .ok()?
            .max(0) as usize;
        let player_index = self.player_index(&holder).ok()?;
        let filter = expression.get("where");
        Some(
            graveyard_end_order(&self.state.players[player_index].graveyard, end)
                .filter(|card| {
                    filter.is_none_or(|filter| {
                        self.runtime_card_filter_matches(
                            &card.definition,
                            filter,
                            stack_object,
                            bindings,
                            decisions,
                        )
                    })
                })
                .take(count)
                .map(|card| card.instance_id.clone())
                .collect(),
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "graveyard_position_tests.rs"]
mod tests;
