use super::*;

fn permission(
    count: NumericExpression,
    duration: EffectDurationNode,
) -> TemporaryPlayerPermissionNode {
    TemporaryPlayerPermissionNode {
        modifier: PlayerModifierNode::AdditionalLandPlay {
            player: LocalPlayerReference::Controller,
            count,
        },
        duration,
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
    node.modifier = PlayerModifierNode::AdditionalLandPlay {
        player: LocalPlayerReference::ControllerOf(TargetBinding {
            effect_index: 0,
            slot_index: 0,
        }),
        count: NumericExpression::Atom(NumericAtom::Integer(1)),
    };
    assert_eq!(
        lower_temporary_player_permission(&node),
        Err(TargetlessLoweringError::RequiresTarget)
    );
}
