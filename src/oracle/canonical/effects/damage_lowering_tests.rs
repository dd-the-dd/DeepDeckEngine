use super::*;

#[test]
fn targeted_damage_lowering_keeps_declaration_and_recipient_id_together() {
    let damage = DamageEffectNode {
        source: LocalObjectReference::SourceSelf,
        amount: NumericAtom::Integer(1),
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

#[test]
fn amass_lowering_emits_the_existing_atomic_runtime_contract() {
    let effect = LocalEffectNode::Amass(AmassEffectNode {
        player: LocalPlayerReference::Controller,
        army_subtype: AmassArmySubtype::Orc,
        count: NumericExpression::Atom(NumericAtom::Integer(1)),
    });

    let lowered = lower_targetless_local_effect(&effect, TargetlessDeclarationSupport::None)
        .expect("amass is targetless and needs no declaration");

    assert_eq!(
        lowered,
        json!({
            "kind": "amass",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "armySubtype": "Orc",
            "count": { "kind": "integer", "value": 1 },
        })
    );
}
