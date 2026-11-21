use super::super::*;

pub(in crate::oracle::canonical) fn lower_choice_binding(binding: ChoiceBinding) -> &'static str {
    match binding {
        ChoiceBinding::BasicLandType => "basicLandType",
        ChoiceBinding::CardName => "chosenCardName",
        // Binding identifiers are opaque links, not a second representation
        // of the parsed numeric amount. `OptionalLifePaymentNode.amount` owns
        // the actual cost while this stable name links the choice to uses.
        ChoiceBinding::OptionalLifePayment => "optionalLifePayment",
    }
}

pub(in crate::oracle::canonical) fn lower_local_condition(condition: &LocalConditionNode) -> Value {
    match condition {
        LocalConditionNode::WasKicked { spell } => json!({
            "kind": "wasKicked",
            "spell": match spell {
                LocalObjectReference::SourceSelf => self_ref(),
            },
        }),
        LocalConditionNode::NotYourTurn => json!({ "kind": "notYourTurn" }),
        LocalConditionNode::CostNotPaid { payment } => not(json!({
            "kind": "costWasPaid",
            "decisionId": lower_choice_binding(*payment),
        })),
        LocalConditionNode::PermanentLeftBattlefieldThisTurn { player } => {
            debug_assert_eq!(*player, LocalPlayerReference::Controller);
            compare(
                ">=",
                json!({
                    "kind": "countEventsThisTurn",
                    "event": "permanentLeftBattlefield",
                    "player": controller(),
                    "where": Value::Null,
                }),
                integer(1),
            )
        }
        LocalConditionNode::CardsExistInZone {
            player,
            zone,
            predicates,
        } => {
            debug_assert_eq!(*player, LocalPlayerReference::Controller);
            debug_assert_eq!(*zone, PlayerZoneKind::Graveyard);
            and(predicates
                .iter()
                .map(|predicate| {
                    compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": lower_card_object_predicate(predicate),
                        }),
                        integer(1),
                    )
                })
                .collect())
        }
        LocalConditionNode::ControlledPermanentCount(condition) => {
            debug_assert_eq!(condition.player, LocalPlayerReference::Controller);
            compare(
                match condition.direction {
                    ControlledCountDirection::OrMore => ">=",
                    ControlledCountDirection::OrFewer => "<=",
                },
                json!({
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": lower_permanent_object_predicate(&condition.predicate),
                    "excludeSource": condition.exclude_source,
                }),
                integer(condition.threshold),
            )
        }
    }
}
