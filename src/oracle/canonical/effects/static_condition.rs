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
            SourceStateProposition::Modified => json!({ "kind": "sourceIsModified" }),
        },
        StaticConditionAtom::ControllerEventCount { event, threshold } => compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": match event {
                    ControllerTurnEvent::CardDrawn => "cardDrawn",
                    ControllerTurnEvent::SpellCast => "spellCast",
                },
                "player": controller(),
                "where": Value::Null,
            }),
            integer(*threshold),
        ),
        StaticConditionAtom::ControllerGainedLifeThisTurn => compare(
            ">=",
            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
            integer(1),
        ),
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
    }
}
