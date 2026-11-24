use super::super::super::{
    CollectionSelectionMethod, DiscardQuantity, NumericAtom, NumericExpression, PlayerBinding,
    PlayerDiscardEffectNode, PlayerEffectSubject, PlayerPredicate, TargetCardinality,
    TargetListNode, TargetSelector, TargetSlot,
};
use super::{
    BoundPlayerLowering, PlayerDiscardLoweringError, PlayerDiscardLoweringIds,
    lower_player_discard_effect,
};

fn ids() -> PlayerDiscardLoweringIds<'static> {
    PlayerDiscardLoweringIds {
        target_player: "discardPlayer",
        random_selection: "randomDiscardCards",
    }
}

fn count(value: i64) -> NumericExpression {
    NumericExpression::Atom(NumericAtom::Integer(value))
}

#[test]
fn chosen_controller_discard_lowers_directly_to_discard_cards() {
    let node = PlayerDiscardEffectNode {
        player: PlayerEffectSubject::Controller,
        quantity: DiscardQuantity::Count(count(1)),
        selection: CollectionSelectionMethod::Choice,
    };

    let lowered = lower_player_discard_effect(&node, ids(), None)
        .expect("ordinary discard uses the existing primitive");
    assert!(lowered.decisions.is_empty());
    assert_eq!(lowered.effects.len(), 1);
    assert_eq!(lowered.effects[0]["kind"], "discardCards");
    assert_eq!(lowered.effects[0]["count"]["value"], 1);
    assert!(lowered.effects[0].get("selectionProvenance").is_none());
}

#[test]
fn targeted_random_discard_composes_random_selection_and_linked_consumer() {
    let binding = PlayerBinding(4);
    let node = PlayerDiscardEffectNode {
        player: PlayerEffectSubject::DeclaredTarget {
            target: TargetListNode {
                slots: vec![TargetSlot {
                    selector: TargetSelector::Player(PlayerPredicate::Opponent),
                    cardinality: TargetCardinality::EXACTLY_ONE,
                    exclude_source: false,
                    count: None,
                }],
            },
            binding,
        },
        quantity: DiscardQuantity::Count(count(2)),
        selection: CollectionSelectionMethod::Random,
    };

    let lowered = lower_player_discard_effect(&node, ids(), None)
        .expect("random target-player discard composes existing primitives");
    assert_eq!(lowered.decisions.len(), 1);
    assert_eq!(
        lowered.declared_player,
        Some((binding, "discardPlayer".to_string()))
    );
    assert_eq!(lowered.effects.len(), 2);
    let selection = &lowered.effects[0];
    let discard = &lowered.effects[1];
    assert_eq!(selection["kind"], "chooseCards");
    assert_eq!(selection["selection"]["kind"], "random");
    assert_eq!(selection["candidates"]["zone"]["kind"], "hand");
    assert_eq!(selection["minimum"]["value"], 2);
    assert_eq!(discard["kind"], "discardCards");
    assert_eq!(discard["cards"]["decisionId"], "randomDiscardCards");
    assert_eq!(
        discard["selectionProvenance"]["decisionId"],
        "randomDiscardCards"
    );
    assert_eq!(selection["player"], discard["player"]);
}

#[test]
fn bound_player_reuses_the_exact_existing_target_id() {
    let binding = PlayerBinding(9);
    let node = PlayerDiscardEffectNode {
        player: PlayerEffectSubject::Bound(binding),
        quantity: DiscardQuantity::Count(NumericExpression::Atom(NumericAtom::VariableX)),
        selection: CollectionSelectionMethod::Random,
    };

    let lowered = lower_player_discard_effect(
        &node,
        ids(),
        Some(BoundPlayerLowering {
            binding,
            target_id: "priorPlayer",
        }),
    )
    .expect("bound player remains linked to the prior target");
    assert!(lowered.decisions.is_empty());
    assert_eq!(lowered.effects[0]["player"]["id"], "priorPlayer");
    assert_eq!(lowered.effects[0]["minimum"]["decisionId"], "xValue");
    assert_eq!(lowered.effects[1]["player"]["id"], "priorPlayer");
}

#[test]
fn mismatched_bound_player_is_not_retargeted() {
    let node = PlayerDiscardEffectNode {
        player: PlayerEffectSubject::Bound(PlayerBinding(3)),
        quantity: DiscardQuantity::Count(count(1)),
        selection: CollectionSelectionMethod::Choice,
    };

    assert_eq!(
        lower_player_discard_effect(
            &node,
            ids(),
            Some(BoundPlayerLowering {
                binding: PlayerBinding(8),
                target_id: "otherPlayer",
            }),
        ),
        Err(PlayerDiscardLoweringError::BoundPlayerMismatch)
    );
}

#[test]
fn whole_hand_discard_lowers_to_the_engine_discard_hand_action() {
    let node = PlayerDiscardEffectNode {
        player: PlayerEffectSubject::Controller,
        quantity: DiscardQuantity::WholeHand,
        selection: CollectionSelectionMethod::Choice,
    };
    let lowered = lower_player_discard_effect(&node, ids(), None).unwrap();
    assert_eq!(
        lowered.effects,
        vec![serde_json::json!({
            "kind": "discardHand",
            "player": {"kind": "controllerOf", "object": {"kind": "self"}},
        })]
    );
}
