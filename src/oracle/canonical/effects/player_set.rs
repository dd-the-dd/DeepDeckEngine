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
        }
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
