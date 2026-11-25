//! Numeric quantities read from game records and card collections that the
//! Oracle numeric language composes (`NumericQuantity`). One evaluator per
//! kind, shared by the controller-relative path (static abilities,
//! intervening conditions) and the resolution path (players resolved with
//! the stack object's bindings):
//!
//! ```text
//! { kind: commanderCastsFromCommandZone, player }   CR 903.8 record
//! { kind: countOpponentsAttackedThisTurn, player }  CR 506.2, every combat
//! { kind: countDistinctManaValues, zone, where, allPlayers? }  CR 202.3
//!   zone: graveyard | hand | library (printed card filter) | battlefield
//! { kind: countManaSymbolsAmongCards, zone, where, color, allPlayers? }
//! { kind: countPlayers, players: filterPlayers }    CR 102.1-102.3 members
//! ```
//!
//! A shape outside this grammar is refused by `numeric_quantity_supported`;
//! an unresolvable player makes the value `None`, never zero.
//!
//! The module also owns the cost-object mana-value binding (`bindManaValueAs`
//! on a sacrifice cost, CR 601.2h, 608.2h): the value is read as last known
//! information when the cost is paid.
use super::*;

/// The zones a collection quantity reads: a card zone filtered on printed
/// characteristics, or the battlefield filtered on permanent state.
fn zone_collection_supported(expression: &Value) -> bool {
    let filter = match value_kind(&expression["zone"]) {
        Some("graveyard" | "hand" | "library") => card_filter_supported(&expression["where"]),
        Some("battlefield") => permanent_filter_supported(&expression["where"]),
        _ => false,
    };
    filter
        && player_reference_supported(&expression["zone"]["player"])
        && expression.get("allPlayers").is_none_or(Value::is_boolean)
}

/// `Some(valid)` for a kind this module owns, `None` otherwise.
pub(super) fn numeric_quantity_supported(expression: &Value) -> Option<bool> {
    if let Some(supported) = state_condition_atoms::player_quantity_supported(expression) {
        return Some(supported);
    }
    if let Some(supported) = resolution_records::resolution_record_supported(expression) {
        return Some(supported);
    }
    let supported = match value_kind(expression) {
        Some("commanderCastsFromCommandZone" | "countOpponentsAttackedThisTurn") => {
            player_reference_supported(&expression["player"])
        }
        Some("countDistinctManaValues") => zone_collection_supported(expression),
        Some("countManaSymbolsAmongCards") => {
            zone_collection_supported(expression)
                && matches!(
                    expression["color"].as_str(),
                    Some("W" | "U" | "B" | "R" | "G")
                )
        }
        // A bare `countPlayers` keeps its own evaluator; a population is
        // counted here (`player_query.rs`).
        Some("countPlayers") if expression.get("players").is_some() => {
            super::player_query::countable_population_supported(&expression["players"])
        }
        _ => return None,
    };
    Some(supported)
}

/// Whether a mana symbol is of the color `symbol` (CR 107.4, 202.2): a
/// hybrid or Phyrexian symbol of that color counts once.
fn mana_symbol_has_color(mana: &str, symbol: &str) -> bool {
    mana.split('/').any(|part| part == symbol)
}

/// The mana value a sacrifice cost records (`bindManaValueAs`), read on the
/// permanent before it leaves (CR 608.2h).
pub(super) fn cost_mana_value_binding(
    cost: &Value,
    permanent: &CardInstance,
) -> Option<(String, i32)> {
    cost["bindManaValueAs"]
        .as_str()
        .map(|binding| (binding.to_string(), mana_value(&permanent.definition)))
}

impl GameEngine {
    /// The cards of the collection's zone for `player_id`, or for every
    /// player still in the game with `allPlayers`, that satisfy `where`.
    fn zone_collection_cards(
        &self,
        expression: &Value,
        player_id: &str,
    ) -> Option<Vec<&CardInstance>> {
        let zone = value_kind(&expression["zone"])?;
        let all_players = expression["allPlayers"].as_bool() == Some(true);
        let mut cards = Vec::new();
        for player in &self.state.players {
            if all_players {
                if player.has_lost {
                    continue;
                }
            } else if player.id != player_id {
                continue;
            }
            let (zone_cards, battlefield) = match zone {
                "hand" => (&player.hand, false),
                "graveyard" => (&player.graveyard, false),
                "library" => (&player.library, false),
                "battlefield" => (&player.battlefield, true),
                _ => return None,
            };
            cards.extend(zone_cards.iter().filter(|card| {
                if battlefield {
                    self.permanent_state_matches(card, &expression["where"])
                } else {
                    matches_card_filter(&card.definition, &expression["where"])
                }
            }));
        }
        if !all_players && self.player_index(player_id).is_err() {
            return None;
        }
        Some(cards)
    }

    /// The opponents `player_id` attacked with at least one creature during
    /// this turn (CR 506.2): attacking a planeswalker or a battle is not
    /// attacking its player.
    fn opponents_attacked_this_turn(&self, player_id: &str) -> i32 {
        let mut defenders = BTreeSet::new();
        for event in &self.state.events {
            if event.turn_number != self.state.turn_number
                || !matches!(event.kind.as_str(), "attackerDeclared" | "declaredAttacker")
                || event.player_id.as_deref() != Some(player_id)
            {
                continue;
            }
            if let Ok(TargetRef::Player {
                player_id: defender,
            }) = serde_json::from_value::<TargetRef>(event.detail["defender"].clone())
                && defender != player_id
            {
                defenders.insert(defender);
            }
        }
        i32::try_from(defenders.len()).unwrap_or(i32::MAX)
    }

    /// `Some(value)` for a kind this module owns, evaluated for the player
    /// the expression names (already resolved to `player_id`).
    pub(super) fn numeric_quantity(
        &self,
        expression: &Value,
        player_id: &str,
    ) -> Option<Option<i32>> {
        numeric_quantity_supported(expression)?;
        if let Some(value) = self.player_quantity(expression, player_id) {
            return Some(value);
        }
        if let Some(value) = self.controller_relative_record(expression, player_id) {
            return Some(value);
        }
        let value = match value_kind(expression) {
            Some("commanderCastsFromCommandZone") => {
                let casts = self
                    .state
                    .commanders
                    .iter()
                    .filter(|commander| commander.owner_id == player_id)
                    .map(|commander| u64::from(commander.casts_from_command_zone))
                    .sum::<u64>();
                i32::try_from(casts).ok()
            }
            Some("countOpponentsAttackedThisTurn") => {
                Some(self.opponents_attacked_this_turn(player_id))
            }
            Some("countPlayers") => self.population_member_count(&expression["players"], player_id),
            Some("countDistinctManaValues") => self
                .zone_collection_cards(expression, player_id)
                .and_then(|cards| {
                    i32::try_from(
                        cards
                            .iter()
                            .map(|card| mana_value(&card.definition))
                            .collect::<BTreeSet<_>>()
                            .len(),
                    )
                    .ok()
                }),
            Some("countManaSymbolsAmongCards") => {
                let symbol = expression["color"].as_str()?;
                self.zone_collection_cards(expression, player_id)
                    .and_then(|cards| {
                        i32::try_from(
                            cards
                                .iter()
                                .flat_map(|card| parse_mana_symbols(&card.definition.mana_cost))
                                .filter(|mana| mana_symbol_has_color(mana, symbol))
                                .count(),
                        )
                        .ok()
                    })
            }
            _ => return None,
        };
        Some(value)
    }

    /// The resolution path: the expression's player (or its zone's holder)
    /// is resolved with the stack object's bindings; without a player
    /// reference the stack object's controller reads it.
    pub(super) fn runtime_numeric_quantity(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<Option<i32>> {
        // A controller-relative count without its own resolution arm: the
        // counters of one kind on one player's permanents (CR 122.1).
        let delegated = value_kind(expression) == Some("countCountersOnPermanents");
        if !delegated {
            numeric_quantity_supported(expression)?;
        }
        let reference = expression
            .get("player")
            .or_else(|| expression.get("zone").and_then(|zone| zone.get("player")));
        let player_id = match reference {
            Some(reference) => match self.resolve_player(reference, stack_object, bindings) {
                Some(player_id) => player_id,
                None => return Some(None),
            },
            None => stack_object.controller.clone(),
        };
        if delegated {
            return Some(self.numeric_expression(expression, &player_id));
        }
        self.numeric_quantity(expression, &player_id)
    }
}

/// The counters a permanent has of one kind, or of every kind (CR 122.1).
pub(super) fn counter_total(counters: &BTreeMap<String, i32>, counter: Option<&str>) -> i32 {
    match counter {
        Some(counter) => counters.get(counter).copied().unwrap_or(0),
        None => counters.values().copied().fold(0_i32, i32::saturating_add),
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "numeric_quantities_tests.rs"]
mod tests;
