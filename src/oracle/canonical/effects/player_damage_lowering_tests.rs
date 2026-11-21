use super::*;

fn effect(amount: NumericAtom, players: PlayerSetReferenceNode) -> LocalEffectNode {
    LocalEffectNode::DealDamage(DamageEffectNode {
        source: crate::oracle::canonical::automata::PermanentOperandNode::Reference(
            LocalObjectReference::SourceSelf,
        ),
        amount: NumericExpression::Atom(amount),
        recipient: DamageRecipientNode::PlayerSet(players),
    })
}

#[test]
fn population_damage_is_one_ordinary_primitive_without_player_binding() {
    let rule = lower_targetless_local_effect(
        &effect(
            NumericAtom::Integer(3),
            PlayerSetReferenceNode::OpponentsOfAbilityController,
        ),
        TargetlessDeclarationSupport::None,
    )
    .unwrap();
    assert_eq!(
        rule,
        json!({"kind":"dealDamage", "source":self_ref(), "amount":{"kind":"integer","value":3},
            "recipient":{"kind":"opponentsOf","player":controller()}})
    );
    let all = lower_targetless_local_effect(
        &effect(NumericAtom::Integer(1), PlayerSetReferenceNode::EachPlayer),
        TargetlessDeclarationSupport::None,
    )
    .unwrap();
    assert_eq!(all["recipient"], json!({"kind":"eachPlayer"}));
}

#[test]
fn shared_numeric_support_never_invents_a_missing_producer_or_truncates() {
    let node = effect(
        NumericAtom::VariableX,
        PlayerSetReferenceNode::OpponentsOfAbilityController,
    );
    assert!(lower_targetless_local_effect(&node, TargetlessDeclarationSupport::None).is_err());
    assert!(lower_targetless_local_effect(&node, TargetlessDeclarationSupport::XValue).is_ok());
    assert!(
        lower_targetless_local_effect(
            &effect(
                NumericAtom::Integer(i64::MAX),
                PlayerSetReferenceNode::EachPlayer
            ),
            TargetlessDeclarationSupport::None
        )
        .is_err()
    );
}

#[test]
fn range_error_in_damage_family_is_a_typed_failure_instead_of_unreachable() {
    let tokens = crate::oracle::syntax::recognize_entities(
        "This spell deals 9223372036854775807 damage to each opponent and you gain 1 life.",
    );
    let mut cursor = OracleTokenCursor::new("damage-player-set-range", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    assert!(matches!(
        lower_local_effect_sequence(sequence, cursor.transitions()),
        Err(DamageFamilySequenceLoweringError::SharedPlayerSetSupport(_))
    ));
}
