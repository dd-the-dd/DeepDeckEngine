//! Lowering of the player clauses of `player_designation.rs` and
//! `player_action.rs` whose primitive is a player's rules state: a player
//! designation (CR 724, 725), the Ring's temptation (CR 701.54), a skipped
//! turn part (CR 500.11) and a prohibition until end of turn (CR 101.2).
//! The acting player is the lowered player subject; a performer is put on
//! the primitive by the sequence.
use super::super::*;

/// `becomes the monarch` / `takes the initiative` / `the Ring tempts you`.
pub(in crate::oracle::canonical) fn lower_player_designation_action(
    action: PlayerDesignationAction,
    player: Value,
) -> Value {
    match action {
        PlayerDesignationAction::Acquire(PlayerDesignation::Monarch) => {
            json!({ "kind": "becomeMonarch", "player": player })
        }
        PlayerDesignationAction::Acquire(PlayerDesignation::Initiative) => {
            json!({ "kind": "takeInitiative", "player": player })
        }
        PlayerDesignationAction::RingTemptation => {
            json!({ "kind": "temptWithRing", "player": player })
        }
    }
}

/// `skip your next [N] PART [this turn]`: the next `count` occurrences of
/// the player's `part` do not happen.
pub(in crate::oracle::canonical) fn lower_skip_turn_part(
    skip: SkipTurnPartNode,
    player: Value,
) -> Value {
    json!({
        "kind": "skipTurnPart",
        "player": player,
        "part": skip.part.name(),
        "count": skip.count,
        "thisTurn": skip.this_turn,
    })
}

/// `can't cast spells this turn`: the shared player action restriction.
pub(in crate::oracle::canonical) fn lower_player_prohibition(
    prohibition: PlayerProhibition,
    player: Value,
) -> Value {
    match prohibition {
        PlayerProhibition::CastSpells => json!({
            "kind": "restrictPlayerActions",
            "player": player,
            "castSpells": true,
            "activateAbilities": false,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }),
    }
}
