use super::*;

#[test]
fn targeted_damage_lowering_keeps_declaration_and_recipient_id_together() {
    let damage = DamageEffectNode {
        source: crate::oracle::canonical::automata::PermanentOperandNode::Reference(
            LocalObjectReference::SourceSelf,
        ),
        amount: NumericExpression::Atom(NumericAtom::Integer(1)),
        recipient: DamageRecipientNode::Targets(TargetListNode {
            slots: vec![TargetSlot {
                selector: TargetSelector::Any,
                cardinality: TargetCardinality::EXACTLY_ONE,
                exclude_source: false,
            }],
        }),
    };

    let lowered = lower_targeted_damage_effect(&damage, "triggerDamageTarget")
        .expect("targeted damage lowers");

    assert_eq!(lowered.decision["kind"], "chooseTargets");
    assert_eq!(lowered.decision["id"], "triggerDamageTarget");
    assert_eq!(
        lowered.decision["candidates"],
        json!({ "kind": "anyTarget" })
    );
    assert_eq!(lowered.effect["kind"], "dealDamage");
    assert_eq!(
        lowered.effect["recipient"],
        json!({ "kind": "chosenTarget", "id": "triggerDamageTarget" })
    );
}
