use super::super::*;

/// Lowers the static-ability condition Boolean program to the engine's
/// condition IR (`condition` on the static rule, evaluated continuously with
/// the ability's source and controller).
pub(in crate::oracle::canonical) fn lower_static_condition(
    condition: &StaticConditionNode,
) -> Value {
    match condition {
        BooleanConditionNode::Atom(atom) => lower_static_condition_atom(atom),
        BooleanConditionNode::And(operands) => {
            and(operands.iter().map(lower_static_condition).collect())
        }
        BooleanConditionNode::Or(operands) => {
            or(operands.iter().map(lower_static_condition).collect())
        }
        BooleanConditionNode::Not(operand) => not(lower_static_condition(operand)),
    }
}

fn lower_static_condition_atom(atom: &StaticConditionAtom) -> Value {
    match atom {
        StaticConditionAtom::Local(local) => lower_local_condition(local),
        StaticConditionAtom::SourceState(state) => match state {
            SourceStateProposition::Tapped => json!({ "kind": "isTapped", "object": self_ref() }),
            SourceStateProposition::Untapped => {
                not(json!({ "kind": "isTapped", "object": self_ref() }))
            }
            SourceStateProposition::Attacking => {
                json!({ "kind": "isAttacking", "object": self_ref() })
            }
            SourceStateProposition::Blocking => {
                json!({ "kind": "isBlocking", "object": self_ref() })
            }
            SourceStateProposition::Modified => json!({ "kind": "sourceIsModified" }),
            SourceStateProposition::Designated(designation) => json!({
                "kind": "sourceFlagSet",
                "flag": designation.flag(),
            }),
        },
        StaticConditionAtom::PlayerState(node) => lower_player_state_condition(node),
        StaticConditionAtom::ControllerTurn(proposition) => {
            let turn = json!({ "kind": "duringControllerTurn", "player": controller() });
            if proposition.is_controller_turn {
                turn
            } else {
                not(turn)
            }
        }
        StaticConditionAtom::ControllerGainedLifeThisTurn { threshold } => compare(
            ">=",
            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
            integer(*threshold),
        ),
        StaticConditionAtom::SourceStatus(predicate) => json!({
            "kind": "objectMatchesFilter",
            "object": self_ref(),
            "where": lower_permanent_object_predicate(predicate),
        }),
        StaticConditionAtom::SourceCounters(presence) => json!({
            "kind": "objectMatchesFilter",
            "object": self_ref(),
            "where": lower_permanent_object_predicate(&ObjectPredicate::HasCounter(
                presence.clone()
            )),
        }),
        StaticConditionAtom::ControllerAttackedThisTurn => compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "attackerDeclared",
                "player": controller(),
                "where": Value::Null,
            }),
            integer(1),
        ),
        StaticConditionAtom::ControllerLostLifeThisTurn => compare(
            ">=",
            json!({ "kind": "lifeLostThisTurn", "player": controller() }),
            integer(1),
        ),
        StaticConditionAtom::OpponentLostLifeThisTurn => compare(
            ">=",
            json!({ "kind": "countOpponentsWhoLostLifeThisTurn", "player": controller() }),
            integer(1),
        ),
        StaticConditionAtom::OpponentDealtDamageThisTurn => compare(
            ">=",
            json!({ "kind": "countOpponentsDealtDamageThisTurn", "player": controller() }),
            integer(1),
        ),
        StaticConditionAtom::OpponentControlledCountComparison {
            predicate,
            relation,
        } => {
            let filter = lower_permanent_object_predicate(predicate);
            let controller_count = json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": filter.clone(),
            });
            // The greatest opponent count decides both quantified relations:
            // some opponent exceeds a value iff the greatest one does.
            let greatest_opponent_count = json!({
                "kind": "greatestOpponentPermanentCount",
                "player": controller(),
                "where": filter,
            });
            match relation {
                OpponentCountRelation::SomeOpponentExceedsController => {
                    compare(">", greatest_opponent_count, controller_count)
                }
                OpponentCountRelation::ControllerExceedsEveryOpponent => {
                    compare(">", controller_count, greatest_opponent_count)
                }
            }
        }
        StaticConditionAtom::PermanentsDiedThisTurn { threshold, subject } => compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "permanentDied",
                "where": lower_card_object_predicate(subject),
            }),
            integer(*threshold),
        ),
        StaticConditionAtom::SpellsCastLastTurn(reading) => {
            let (aggregate, operator, bound) = match reading {
                LastTurnSpellCasts::None => ("total", "==", 0),
                LastTurnSpellCasts::SomePlayerAtLeast(threshold) => {
                    ("greatestPerPlayer", ">=", *threshold)
                }
            };
            compare(
                operator,
                json!({
                    "kind": "countEventsPreviousTurn",
                    "event": "spellCast",
                    "aggregate": aggregate,
                }),
                integer(bound),
            )
        }
        StaticConditionAtom::CardsInControllerGraveyard {
            threshold,
            direction,
            predicate,
        } => compare(
            match direction {
                ControlledCountDirection::OrMore => ">=",
                ControlledCountDirection::OrFewer => "<=",
            },
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": lower_card_object_predicate(predicate),
            }),
            integer(*threshold),
        ),
        StaticConditionAtom::ControllerHasEnduringStory => {
            json!({ "kind": "hasEnduringStory", "player": controller() })
        }
        StaticConditionAtom::ControllerCompletedDungeon { dungeon, completed } => {
            let mut atom = json!({ "kind": "completedDungeon", "player": controller() });
            if let Some(dungeon) = dungeon {
                atom["name"] = json!(dungeon);
            }
            if *completed {
                atom
            } else {
                json!({ "kind": "not", "operand": atom })
            }
        }
    }
}
