use super::*;

fn permission(
    count: NumericExpression,
    duration: EffectDurationNode,
) -> TemporaryPlayerPermissionNode {
    TemporaryPlayerPermissionNode {
        rule: TemporaryRuleNode::Allowance(PlayerModifierNode::AdditionalLandPlay {
            player: LocalPlayerReference::Controller,
            count,
        }),
        duration: TemporaryRuleDuration::Effect(duration),
    }
}

#[test]
fn current_turn_allowance_lowers_only_the_existing_primitive() {
    for count in [1, 2] {
        let node = permission(
            NumericExpression::Atom(NumericAtom::Integer(count)),
            EffectDurationNode::UntilEndOfCurrentTurn,
        );
        assert_eq!(
            lower_temporary_player_permission(&node).unwrap(),
            json!({
                "kind":"grantAdditionalLandPlays", "player":controller(), "count":integer(count),
                "duration":{"kind":"untilEndOfCurrentTurn"},
            })
        );
    }
}

#[test]
fn unsupported_amount_lifetime_and_actor_never_emit_default_values() {
    for count in [
        NumericExpression::Atom(NumericAtom::Integer(0)),
        NumericExpression::Atom(NumericAtom::Integer(-1)),
        NumericExpression::Atom(NumericAtom::Integer(i64::from(i32::MAX) + 1)),
        NumericExpression::Atom(NumericAtom::VariableX),
        NumericExpression::add(
            NumericExpression::Atom(NumericAtom::Integer(1)),
            NumericExpression::Atom(NumericAtom::Integer(2)),
        ),
    ] {
        assert!(
            lower_temporary_player_permission(&permission(
                count,
                EffectDurationNode::UntilEndOfCurrentTurn
            ))
            .is_err()
        );
    }
    assert!(
        lower_temporary_player_permission(&permission(
            NumericExpression::Atom(NumericAtom::Integer(1)),
            EffectDurationNode::UntilNextTurn {
                player: LocalPlayerReference::Controller
            }
        ))
        .is_err()
    );
    let mut node = permission(
        NumericExpression::Atom(NumericAtom::Integer(1)),
        EffectDurationNode::UntilEndOfCurrentTurn,
    );
    node.rule = TemporaryRuleNode::Allowance(PlayerModifierNode::AdditionalLandPlay {
        player: LocalPlayerReference::ControllerOf(TargetBinding {
            effect_index: 0,
            slot_index: 0,
        }),
        count: NumericExpression::Atom(NumericAtom::Integer(1)),
    });
    assert_eq!(
        lower_temporary_player_permission(&node),
        Err(TargetlessLoweringError::RequiresTarget)
    );
}

/// CR 702.8a: a temporary flash rule installs the static flash grant for
/// the rule's lifetime; the spell query is the shared spell filter.
#[test]
fn temporary_flash_rule_installs_the_static_flash_grant() {
    let node = TemporaryPlayerPermissionNode {
        rule: TemporaryRuleNode::FlashCasting {
            spells: ObjectPredicate::CardType(ObjectCardType::Creature),
        },
        duration: TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn),
    };
    let lowered = lower_temporary_player_permission(&node).expect("flash rule lowers");
    assert_eq!(lowered["kind"], "installTemporaryStaticModifiers");
    assert_eq!(lowered["duration"]["kind"], "untilEndOfCurrentTurn");
    let grant = &lowered["modifiers"][0];
    assert_eq!(grant["kind"], "grantFlashCasting");
    assert_eq!(grant["player"], controller());
    assert_eq!(grant["firstEachTurn"], false);
    assert!(!grant["where"].is_null());
}
