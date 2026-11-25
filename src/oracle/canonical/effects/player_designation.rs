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
        PlayerDesignationAction::Acquire(AcquiredDesignation::Monarch) => {
            json!({ "kind": "becomeMonarch", "player": player })
        }
        PlayerDesignationAction::Acquire(AcquiredDesignation::Initiative) => {
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

/// A player's own status change: `loses the game` (CR 104.3a), or one
/// `grantPlayerKeyword` per granted keyword with the stated duration
/// (CR 702.11d hexproof, 702.18d shroud). Protection from everything is the
/// shared player protection primitive, until the controller's next turn
/// (CR 702.16k). The failure names the unsupported duration.
pub(in crate::oracle::canonical) fn lower_player_status_action(
    status: &PlayerStatusAction,
    player: Value,
) -> Result<Vec<Value>, &'static str> {
    let PlayerStatusAction::GainKeywords(grant) = status else {
        return Ok(vec![json!({ "kind": "loseGame", "player": player })]);
    };
    let duration = match &grant.duration {
        EffectDurationNode::UntilEndOfCurrentTurn => json!({ "kind": "untilEndOfCurrentTurn" }),
        EffectDurationNode::UntilNextTurn {
            player: LocalPlayerReference::Controller,
        } => json!({ "kind": "untilNextTurn", "player": controller() }),
        _ => return Err("a player keyword duration: until end of turn or until your next turn"),
    };
    Ok(grant
        .keywords
        .iter()
        .map(|keyword| match keyword {
            PlayerGrantedKeyword::Hexproof | PlayerGrantedKeyword::Shroud => json!({
                "kind": "grantPlayerKeyword",
                "player": player.clone(),
                "keyword": if *keyword == PlayerGrantedKeyword::Hexproof {
                    "hexproof"
                } else {
                    "shroud"
                },
                "duration": duration.clone(),
            }),
            PlayerGrantedKeyword::ProtectionFromEverything => json!({
                "kind": "grantPlayerProtection",
                "player": player.clone(),
                "from": ["everything"],
                "duration": duration.clone(),
            }),
        })
        .collect())
}

/// `distribute N counters among TARGETS` (CR 601.2d): the division of the
/// one declared target slot `objects` names, announced with the targets
/// (each at least one), then the counters each target receives.
pub(in crate::oracle::canonical) fn lower_divided_counters(
    objects: &[Value],
    counter: &str,
    amount: Value,
) -> Option<(Value, Value)> {
    let [object] = objects else {
        return None;
    };
    let targets = object["id"].as_str()?;
    let division = format!("{targets}:division");
    Some((
        json!({
            "kind": "divideQuantityAmongTargets",
            "id": division,
            "targetsDecisionId": targets,
            "quantity": amount,
            "minimumPerTarget": 1,
        }),
        json!({
            "kind": "putDividedCounters",
            "targetsDecisionId": targets,
            "divisionDecisionId": division,
            "counter": counter,
        }),
    ))
}
