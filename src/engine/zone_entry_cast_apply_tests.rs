use super::*;
use rand::RngCore;

fn cast_setup(index: usize, zone: &str) -> (GameEngine, LegalAction, CardInstance) {
    let mut e = engine();
    let barrier = e.state.players[0].battlefield.remove(0);
    let player_id = format!("p{index}");
    let permissions = definition(
        "permission",
        "Enchantment",
        vec![
            active(
                json!({"kind":"castingPermission","players":{"kind":"eachPlayer"},"sourceZone":"graveyard","where":null,"withoutPayingManaCost":false,"asThoughFlash":true}),
            ),
            active(
                json!({"kind":"playCardsFromTopLibrary","player":{"kind":"controllerOf","object":{"kind":"self"}},"where":null}),
            ),
        ],
    );
    assert!(permissions.rules.iter().all(rule_is_executable));
    e.state.players[index]
        .battlefield
        .push(card_instance(&player_id, &permissions, 1));
    let mut spell = definition(
        "spell",
        "Instant",
        vec![
            json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]}),
        ],
    );
    spell.mana_cost = "{1}".into();
    assert!(spell.rules.iter().all(rule_is_executable));
    let card = card_instance(&player_id, &spell, 2);
    let instance_id = card.instance_id.clone();
    match zone {
        "graveyard" => e.state.players[index].graveyard.push(card),
        "library" => e.state.players[index].library.push(card),
        "hand" => e.state.players[index].hand.push(card),
        _ => panic!("test source zone"),
    }
    e.state.players[index].mana_pool.push(FloatingMana {
        symbol: "C".into(),
        spend_restriction: None,
    });
    e.admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let action = e
        .legal_priority_actions(index)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some(instance_id.as_str())
        })
        .expect("a real legal cast action without prohibition");
    assert_eq!(
        action
            .decisions
            .get("castSourceZone")
            .and_then(Value::as_str)
            .unwrap_or("hand"),
        zone
    );
    (e, action, barrier)
}

fn apply_and_resolve(e: &mut GameEngine, index: usize, action: &LegalAction) {
    let before_life = e.state.players[index].life;
    e.apply_priority_action(action, &mut NoChoices).unwrap();
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(e.state.players[index].mana_pool.len(), 0);
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|event| event.kind == "spellCast")
            .count(),
        1
    );
    e.resolve_top_stack(&mut NoChoices).unwrap();
    assert!(e.state.stack.is_empty());
    assert_eq!(e.state.players[index].life, before_life + 1);
    assert!(
        e.state.players[index]
            .graveyard
            .iter()
            .any(|card| card.definition.id == "spell")
    );
}

#[test]
fn cast_apply_rechecks_both_source_zones_before_payment_then_allows_after_source_leaves() {
    for index in 0..2 {
        for zone in ["graveyard", "library"] {
            let (mut e, action, barrier) = cast_setup(index, zone);
            e.state.players[0].battlefield.insert(0, barrier);
            let before = serde_json::to_value(&e.state).unwrap();
            let mut expected_rng = e.rng.clone();
            let error = e
                .apply_priority_action(&action, &mut NoChoices)
                .expect_err("newly active prohibition must reject a previously legal cast");
            assert!(error.message.contains("prohibited"));
            assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
            assert_eq!(e.rng.next_u64(), expected_rng.next_u64());
            e.state.players[0].battlefield.remove(0);
            apply_and_resolve(&mut e, index, &action);
        }
    }
}

#[test]
fn cast_apply_keeps_hand_and_nonmatching_filter_legal() {
    for zone in ["hand", "graveyard", "library"] {
        let (mut e, action, mut barrier) = cast_setup(0, zone);
        if zone != "hand" {
            barrier.definition.rules[1]["modifiers"][0]["where"] =
                json!({"kind":"cardTypeContains","value":"Creature"});
            assert!(barrier.definition.rules.iter().all(rule_is_executable));
        }
        e.state.players[0].battlefield.insert(0, barrier);
        apply_and_resolve(&mut e, 0, &action);
    }
}
