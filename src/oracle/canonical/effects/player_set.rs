//! Canonical populations shared by action parents after their own scope checks.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_player_set_reference(
    players: PlayerSetReferenceNode,
) -> Value {
    match players {
        PlayerSetReferenceNode::EachPlayer => json!({"kind":"eachPlayer"}),
        PlayerSetReferenceNode::OpponentsOfAbilityController => {
            json!({"kind":"opponentsOf", "player":controller()})
        }
        // CR 102.3: the controller's opponents but the triggering player.
        PlayerSetReferenceNode::OpponentsOtherThanEventPlayer => json!({
            "kind": "filterPlayers",
            "players": {"kind":"opponentsOf", "player":controller()},
            "except": {"kind": "triggeringPlayer"},
        }),
    }
}

/// The member of a filtered population inside its predicate: the shared
/// `quantifiedPlayer` reference the runtime binds to each member in turn
/// (as in `playerQuantifier`, CR 102.1-102.3).
fn population_member() -> Value {
    json!({"kind": "quantifiedPlayer"})
}

/// `filterPlayers`: the members of a population that are not the excluded
/// player and satisfy the relative predicate, read when the action begins.
/// `except` is the lowered excluded player, resolved by the caller from
/// its own bindings.
pub(in crate::oracle::canonical) fn lower_player_population_query(
    query: &PlayerPopulationQueryNode,
    except: Option<Value>,
) -> Result<Value, &'static str> {
    if query.except.is_some() != except.is_some() {
        return Err("the lowered player excluded from the population");
    }
    // CR 102.3: `each other player` is every player but the controller.
    let players = if query.excluding_controller {
        json!({"kind": "playersExcept", "player": controller()})
    } else {
        lower_player_set_reference(query.players)
    };
    let mut lowered = json!({"kind": "filterPlayers", "players": players});
    if let Some(except) = except {
        lowered["except"] = except;
    }
    match &query.predicate {
        None => {}
        Some(PlayerRelativePredicate::State {
            proposition,
            negated,
        }) => {
            let (operator, value) = threshold_comparison(proposition.threshold);
            let fact = compare(
                operator.ir_operator(),
                lower_player_measure(&proposition.measure, population_member()),
                integer(value),
            );
            lowered["where"] = if *negated { not(fact) } else { fact };
        }
        // CR 119.3, 120.3: an event of this turn whose player is the member.
        Some(PlayerRelativePredicate::History(history)) => {
            let events = match history {
                MemberHistory::LostLife => {
                    json!({"kind": "lifeLostThisTurn", "player": population_member()})
                }
                MemberHistory::DealtDamage { combat_only } => {
                    let mut events = json!({
                        "kind": "countEventsThisTurn",
                        "event": "damageDealt",
                        "player": population_member(),
                    });
                    if *combat_only {
                        events["detail"] = json!({ "combat": true });
                    }
                    events
                }
            };
            lowered["where"] = compare(">=", events, integer(1));
        }
        Some(PlayerRelativePredicate::Superlative { measure, extremum }) => {
            if query.players != PlayerSetReferenceNode::EachPlayer
                || query.excluding_controller
                || query.except.is_some()
            {
                return Err("a superlative over every player of the game");
            }
            lowered["superlative"] = json!({
                "extremum": match extremum {
                    PlayerExtremum::Greatest => "greatest",
                    PlayerExtremum::Least => "least",
                },
                "value": lower_player_measure(measure, population_member()),
            });
            // `the player with the most life`: only a sole holder.
            if query.unique {
                lowered["superlative"]["unique"] = json!(true);
            }
        }
    }
    if query.unique && lowered.get("superlative").is_none() {
        return Err("a superlative that singles out one player");
    }
    Ok(lowered)
}

/// `the player to your left|right` of the ability's controller (CR 101.4).
pub(in crate::oracle::canonical) fn lower_seat_neighbor(direction: SeatDirection) -> Value {
    json!({
        "kind": "seatNeighbor",
        "player": controller(),
        "direction": match direction {
            SeatDirection::Left => "left",
            SeatDirection::Right => "right",
        },
    })
}

/// The binding a population's member takes when an operand of an action on
/// each member names that member (`actingPlayer`).
const ACTING_PLAYER_BINDING: &str = "actingPlayer";

/// The player slot of a primitive whose operands may name its acting player:
/// the player who gains, loses or sets life, draws, mills, discards or gets
/// counters, and the recipient a damage primitive is dealt to.
fn acting_player_slot(kind: &str) -> Option<&'static str> {
    match kind {
        "gainLife" | "loseLife" | "setLifeTotal" | "drawCards" | "mill" | "discardCards"
        | "addPlayerCounters" => Some("player"),
        "dealDamage" => Some("recipient"),
        _ => None,
    }
}

fn contains_pending_acting_player(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some(ACTING_PLAYER)
                || map.values().any(contains_pending_acting_player)
        }
        Value::Array(items) => items.iter().any(contains_pending_acting_player),
        _ => false,
    }
}

/// A player reference that names several players: the action is performed
/// for each of them in turn.
fn names_player_population(reference: &Value) -> bool {
    matches!(
        reference.get("kind").and_then(Value::as_str),
        Some("eachPlayer" | "opponentsOf" | "playersExcept" | "filterPlayers")
    )
}

/// Substitutes the acting player of every primitive whose operands name it
/// (`BoundPlayerReference::ActingPlayer`, the distributive `they` / `that
/// player`): the primitive's own player slot when it names one player, and,
/// when it names a population, the member of a player loop that performs
/// the primitive once per member in turn order (CR 101.4, 608.2c), so each
/// member's operand reads that member (`each player loses 1 life for each
/// creature they control`, `deals damage to each player equal to the number
/// of lands they control`). Nested primitives bind first; a placeholder no
/// enclosing primitive binds stays in the rule and the runtime refuses it.
pub(in crate::oracle::canonical) fn bind_pending_acting_players(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                bind_pending_acting_players(item);
            }
            return;
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                bind_pending_acting_players(child);
            }
        }
        _ => return,
    }
    let Some(slot) = value
        .get("kind")
        .and_then(Value::as_str)
        .and_then(acting_player_slot)
    else {
        return;
    };
    if !contains_pending_acting_player(value) {
        return;
    }
    let player = value.get(slot).cloned().unwrap_or(Value::Null);
    if player.is_null() || contains_pending_acting_player(&player) {
        return;
    }
    if names_player_population(&player) {
        let member = json!({"kind": "boundValue", "id": ACTING_PLAYER_BINDING});
        let mut body = value.take();
        body[slot] = member.clone();
        bind_acting_player(&mut body, &member);
        *value = json!({
            "kind": "forEachPlayer",
            "players": player,
            "bind": ACTING_PLAYER_BINDING,
            "effects": [body],
        });
    } else {
        bind_acting_player(value, &player);
    }
}
