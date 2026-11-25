use super::*;
use crate::oracle::syntax::recognize_entities;

fn effect(selector: TargetSelector) -> ZoneChangeEffectNode {
    ZoneChangeEffectNode {
        action: ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand),
        target: TargetListNode {
            slots: vec![TargetSlot {
                selector,
                cardinality: TargetCardinality::EXACTLY_ONE,
                exclude_source: false,
                count: None,
            }],
        },
        regeneration: ZoneChangeRegenerationPolicy::Allowed,
        until: None,
    }
}

#[test]
fn mixed_return_candidates_share_opponent_scope_on_spell_and_permanent() {
    let tokens = recognize_entities(
        "Return target spell or nonland permanent an opponent controls to its owner's hand.",
    );
    let mut cursor = OracleTokenCursor::new("return-candidates-real", &tokens);
    let LocalEffectNode::ZoneChange(node) = recognize_zone_change_effect_with_policy(
        &mut cursor,
        ZoneChangeTargetSupport::Scalar,
        ZoneChangeDomains::SpellOrBattlefield,
    )
    .unwrap() else {
        panic!("zone change")
    };
    let (id, candidates, min, max) = lower_zone_change_target_candidates(&node).unwrap();
    assert_eq!(id, "targetObject");
    assert_eq!((min, max), (1, 1));
    assert_eq!(candidates["kind"], "union");
    let sets = candidates["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 2);
    assert_eq!(
        sets[0],
        json!({"kind":"spells","controller":{"kind":"opponentsOf","player":controller()}})
    );
    assert_eq!(
        sets[1],
        json!({"kind":"permanents","controller":{"kind":"opponentsOf","player":controller()},"where":{"kind":"not","operand":{"kind":"cardTypeContains","value":"Land"}}})
    );
}

#[test]
fn independently_qualified_selector_branches_keep_distinct_controllers() {
    let node = effect(TargetSelector::Union(vec![
        TargetSelector::ControlledObject {
            predicate: ObjectPredicate::ObjectKind(ObjectKind::Spell),
            controller: ObjectControllerScope::AbilityController,
        },
        TargetSelector::ControlledObject {
            predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
            controller: ObjectControllerScope::OpponentOfAbilityController,
        },
    ]));
    let (_, candidates, _, _) = lower_zone_change_target_candidates(&node).unwrap();
    assert_eq!(candidates["sets"][0]["controller"], controller());
    assert_eq!(
        candidates["sets"][1]["controller"],
        json!({"kind":"opponentsOf","player":controller()})
    );
}

#[test]
fn permanent_only_candidates_stay_exact_and_invalid_sequence_nodes_fail_safely() {
    let node = effect(TargetSelector::ControlledObject {
        predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
        controller: ObjectControllerScope::OpponentOfAbilityController,
    });
    let old = lower_target_slot_candidates(&node.target.slots[0]).unwrap();
    let new = lower_zone_change_target_candidates(&node).unwrap();
    assert_eq!((new.0, new.1), old);
    let invalid = EffectSequenceNode {
        effects: vec![LocalEffectNode::ZoneChange(effect(TargetSelector::Player(
            PlayerPredicate::Any,
        )))],
        connectors: vec![],
        where_x: vec![],
        conditionals: vec![],
        unless: vec![],
        delayed: vec![],
        iterations: vec![],
    };
    let mut cursor = OracleTokenCursor::new("return-invalid-node", &[]);
    assert!(lower_local_effect_sequence(&mut cursor, &invalid).is_err());
}
