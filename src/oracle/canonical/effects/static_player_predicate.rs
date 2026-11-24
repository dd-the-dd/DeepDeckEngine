//! Lowering of the static frame's player subjects and player predicates
//! (CR 604.1, 611.3) into the engine's battlefield-active player modifiers.
//!
//! Each predicate lowers to one primitive on the same player population;
//! a spell or object qualifier is the shared object-predicate lowering. No
//! predicate is approximated: an action without a runtime primitive fails.
use super::super::*;

/// The engine player reference of a static player subject. An attached
/// permanent's controller requires an Aura or Equipment source.
pub(in crate::oracle::canonical) fn lower_static_player_subject(
    cursor: &OracleTokenCursor<'_>,
    subject: StaticPlayerSubjectNode,
    face_type_line: &str,
) -> MachineResult<Value> {
    Ok(match subject {
        StaticPlayerSubjectNode::You => controller(),
        StaticPlayerSubjectNode::Players(players) => lower_player_set_reference(players),
        StaticPlayerSubjectNode::AttachedPermanentController => {
            cursor.require(
                face_type_line
                    .split(|ch: char| !ch.is_alphanumeric())
                    .any(|word| word == "Aura" || word == "Equipment"),
                "an attached permanent's controller on an Aura or Equipment source",
            )?;
            json!({ "kind": "controllerOfAttachedPermanent", "attachment": self_ref() })
        }
    })
}

/// One player predicate as battlefield-active modifiers on `players`.
pub(in crate::oracle::canonical) fn lower_static_player_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    players: &Value,
    predicate: &StaticPlayerPredicateNode,
) -> MachineResult<Vec<Value>> {
    cursor.enter("static.player.lowering");
    Ok(match predicate {
        StaticPlayerPredicateNode::HasKeywords(keywords) => keywords
            .iter()
            .map(|keyword| {
                json!({
                    "kind": match keyword {
                        PlayerKeywordNode::Hexproof => "playerHexproof",
                        PlayerKeywordNode::Shroud => "playerShroud",
                    },
                    "player": players.clone(),
                })
            })
            .collect(),
        StaticPlayerPredicateNode::Prohibited(actions) => actions
            .iter()
            .map(|action| lower_prohibited_player_action(cursor, players, action))
            .collect::<MachineResult<Vec<_>>>()?,
        StaticPlayerPredicateNode::PlayWithHandsRevealed => {
            vec![json!({ "kind": "revealHands", "players": players.clone() })]
        }
        StaticPlayerPredicateNode::MaximumHandSize(HandSizeChangeNode::Set(amount)) => {
            vec![json!({
                "kind": "maximumHandSize", "player": players.clone(), "amount": integer(*amount),
            })]
        }
        StaticPlayerPredicateNode::MaximumHandSize(HandSizeChangeNode::Adjust(amount)) => {
            vec![json!({
                "kind": "modifyMaximumHandSize",
                "player": players.clone(),
                "amount": integer(*amount),
            })]
        }
        StaticPlayerPredicateNode::AdditionalLandPlay { count } => vec![json!({
            "kind": "additionalLandPlay", "player": players.clone(), "count": integer(*count),
        })],
        StaticPlayerPredicateNode::SkipDrawStep => {
            vec![json!({ "kind": "skipDrawStep", "player": players.clone() })]
        }
    })
}

fn lower_prohibited_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    players: &Value,
    action: &ProhibitedPlayerActionNode,
) -> MachineResult<Value> {
    Ok(match action {
        ProhibitedPlayerActionNode::GainLife => json!({
            "kind": "prohibitPlayerAction", "players": players.clone(), "action": "gainLife",
        }),
        ProhibitedPlayerActionNode::PlayLands => json!({
            "kind": "prohibitPlayerAction", "players": players.clone(), "action": "playLand",
        }),
        ProhibitedPlayerActionNode::LoseGame => {
            json!({ "kind": "cantLoseGame", "player": players.clone() })
        }
        ProhibitedPlayerActionNode::CastSpells { spells, window } => {
            let mut modifier = json!({ "kind": "prohibitCast", "players": players.clone() });
            // A bare `spells` lowers to no filter (the whole spell domain).
            if let Some(filter) = lower_spell_object_predicate(&spells.predicate) {
                modifier["where"] = filter;
            }
            match window {
                None => {}
                Some(CastWindowNode::ControllerTurn) => modifier["duringTurnOf"] = controller(),
                Some(CastWindowNode::Combat) => modifier["duringCombat"] = json!(true),
            }
            modifier
        }
        ProhibitedPlayerActionNode::CastMoreThan { maximum, spells } => {
            let filter = lower_spell_object_predicate(&spells.predicate).unwrap_or(Value::Null);
            cursor.enter("static.player.lowering.cast-quota-filter");
            cursor.require(
                quota_history_filter_supported(&filter),
                "a universal cast-history filter or Boolean filter over recorded types, subtypes, supertypes or names",
            )?;
            json!({
                "kind": "castLimitPerTurn",
                "players": players.clone(),
                "where": filter,
                "maximum": integer(*maximum),
            })
        }
        ProhibitedPlayerActionNode::ActivateAbilities { sources } => {
            cursor.enter("static.player.lowering.activation-sources");
            let sources = cursor.require_value(
                sources.as_ref(),
                "the permanents whose abilities can't be activated (`of OBJECTS`)",
            )?;
            cursor.require(
                sources.predicate.supports_domain(ObjectKind::Permanent),
                "activation sources anchored in the battlefield-permanent domain",
            )?;
            json!({
                "kind": "prohibitActivation",
                "players": players.clone(),
                "abilities": {
                    "kind": "activatedAbilities",
                    "sources": {
                        "kind": "permanents",
                        "where": lower_permanent_object_predicate(&sources.predicate),
                    },
                },
            })
        }
        ProhibitedPlayerActionNode::DrawMoreThan { maximum } => json!({
            "kind": "drawLimitPerTurn", "players": players.clone(), "maximum": integer(*maximum),
        }),
        ProhibitedPlayerActionNode::UntapMoreThan { maximum, objects } => {
            cursor.enter("static.player.lowering.untap-objects");
            cursor.require(
                objects.predicate.supports_domain(ObjectKind::Permanent),
                "untapped objects anchored in the battlefield-permanent domain",
            )?;
            json!({
                "kind": "untapLimitPerUntapStep",
                "players": players.clone(),
                "objects": {
                    "kind": "permanents",
                    "where": lower_permanent_object_predicate(&objects.predicate),
                },
                "maximum": integer(*maximum),
            })
        }
    })
}

#[cfg(test)]
#[path = "static_player_predicate_tests.rs"]
mod tests;
