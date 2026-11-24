use super::*;

fn collection(controller: Option<ObjectControllerScope>) -> PermanentOperandNode {
    PermanentOperandNode::Collection {
        objects: ObjectQueryNode {
            predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
            controller,
            exclude_source: false,
        },
        exclude_source: false,
    }
}

#[test]
fn source_ir_is_exact_and_collection_reuses_the_same_primitive() {
    assert_eq!(
        lower_nontargeted_permanent_operand(&PermanentOperandNode::Reference(
            LocalObjectReference::SourceSelf
        ))
        .unwrap(),
        self_ref()
    );
    let effect = LocalEffectNode::PutCounters(PutCountersOnObjectEffectNode {
        permanent: collection(Some(ObjectControllerScope::AbilityController)),
        counter: crate::oracle::counter_name::CounterName::from_parts(["+1/+1"]).unwrap(),
        count: NumericExpression::from(1),
        direction: Default::default(),
    });
    let rule = lower_targetless_local_effect(&effect, TargetlessDeclarationSupport::None).unwrap();
    assert_eq!(
        rule,
        json!({
            "kind": "putCounters", "counter": "+1/+1", "count": integer(1),
            "permanent": {"kind": "eachPermanent", "player": controller(), "where": card_type("Creature")},
        })
    );
    assert!(
        lower_nontargeted_permanent_operand(&collection(None))
            .unwrap()
            .get("player")
            .is_none()
    );
}

#[test]
fn player_sets_keep_their_population_instead_of_becoming_all_players() {
    // `creatures your opponents control`, `creatures you don't control`,
    // `creatures defending player controls`: the collection keeps the
    // shared controller reference; it never widens to every player.
    for (scope, player) in [
        (
            ObjectControllerScope::OpponentOfAbilityController,
            json!({"kind": "opponentsOf", "player": controller()}),
        ),
        (
            ObjectControllerScope::NotAbilityController,
            json!({"kind": "playersExcept", "player": controller()}),
        ),
        (
            ObjectControllerScope::EventPlayer,
            json!({"kind": "triggeringPlayer"}),
        ),
    ] {
        let lowered = lower_nontargeted_permanent_operand(&collection(Some(scope))).unwrap();
        assert_eq!(lowered["kind"], "eachPermanent");
        assert_eq!(lowered["player"], player);
        assert_eq!(lowered["where"], card_type("Creature"));
    }
}

#[test]
fn unsupported_declaration_dependent_predicate_is_not_replaced_with_zero() {
    let operand = PermanentOperandNode::Collection {
        objects: ObjectQueryNode {
            predicate: ObjectPredicate::Intersection(vec![
                ObjectPredicate::CardType(ObjectCardType::Creature),
                ObjectPredicate::NumericComparison {
                    left: SelectorNumericProperty::Power,
                    operator: NumericComparisonOperator::LessThanOrEqual,
                    right: Box::new(NumericAtom::VariableX.into()),
                },
            ]),
            controller: Some(ObjectControllerScope::AbilityController),
            exclude_source: false,
        },
        exclude_source: false,
    };
    assert_eq!(
        lower_nontargeted_permanent_operand(&operand).unwrap_err(),
        TargetlessLoweringError::UnsupportedCollectionPredicate,
    );
}
