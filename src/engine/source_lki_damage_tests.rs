use super::*;

fn definition(id: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: "Enchantment".into(),
        mana_cost: "{R}".into(),
        power: None,
        toughness: None,
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: vec![],
    }
}
fn position() -> (GameEngine, StackObject) {
    let mut e = GameEngine::new(
        GameSetup {
            players: (0..4)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![definition("filler"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        16411,
    )
    .unwrap();
    let mut source = card_instance("player-0", &definition("source"), 890);
    e.prepare_battlefield_entry_identity(&mut source);
    e.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "pending-damage".into(),
        controller: "player-0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (e, stack)
}
fn effect() -> Value {
    json!({"kind":"dealDamage","source":{"kind":"self"},"amount":{"kind":"integer","value":1},"recipient":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}}})
}
fn grant_lifelink(e: &mut GameEngine, id: &str) {
    e.state.rule_modifiers.push(json!({"kind":"temporaryKeywordUntilTurn","cardInstanceId":id,"keyword":"lifelink","expiresBeforeTurn":e.state.turn_number+1}));
}

#[test]
fn actual_control_change_keeps_ability_controller_but_uses_current_source_traits() {
    let (mut e, stack) = position();
    let id = stack.card.instance_id.clone();
    e.change_permanent_controller(&id, "player-1").unwrap();
    grant_lifelink(&mut e, &id);
    assert!(e.state.players[0].battlefield.is_empty());
    assert_eq!(
        GameEngine::tracked_zone_identity(&e.state.players[1].battlefield[0]),
        GameEngine::tracked_zone_identity(&stack.card)
    );
    e.execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 22, 19, 19]
    );
}
#[test]
fn all_four_central_departures_capture_effective_lifelink_and_last_controller() {
    for destination in ["graveyard", "exile", "hand", "library"] {
        let (mut e, stack) = position();
        let id = stack.card.instance_id.clone();
        e.change_permanent_controller(&id, "player-1").unwrap();
        grant_lifelink(&mut e, &id);
        e.state.stack.push(stack);
        match destination {
            "graveyard" => e.move_permanent_to_graveyard(&id, "sacrificed").unwrap(),
            "exile" => e.exile_permanent(&id).unwrap(),
            "hand" => e.return_permanent_to_hand(&id).unwrap(),
            _ => e.put_permanent_on_owner_library(&id, "top").unwrap(),
        };
        let pending = e.state.stack.pop().unwrap();
        e.state.rule_modifiers.clear();
        let resolved = e.resolve_damage_source_lki(&pending).unwrap();
        assert!(resolved.last_known);
        assert!(resolved.properties.lifelink);
        assert_eq!(resolved.properties.controller_id, "player-1");
        e.execute_player_set_damage(&effect(), &pending, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
        assert_eq!(
            e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
            [20, 22, 19, 19],
            "{destination}"
        );
    }
}
#[test]
fn same_id_returned_under_another_controller_never_replaces_captured_source() {
    let (mut e, stack) = position();
    let id = stack.card.instance_id.clone();
    e.change_permanent_controller(&id, "player-1").unwrap();
    grant_lifelink(&mut e, &id);
    e.state.stack.push(stack);
    e.exile_permanent(&id).unwrap();
    let mut returned = e.state.players[0].exile.pop().unwrap();
    returned.controller = "player-2".into();
    e.prepare_battlefield_entry_identity(&mut returned);
    e.state.players[2].battlefield.push(returned);
    e.state.rule_modifiers.clear();
    let pending = e.state.stack.pop().unwrap();
    let source = e.resolve_damage_source_lki(&pending).unwrap();
    assert!(source.last_known);
    assert_eq!(source.properties.controller_id, "player-1");
    assert!(source.properties.lifelink);
    e.execute_player_set_damage(&effect(), &pending, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 22, 19, 19]
    );
}
#[test]
fn absent_uncaptured_source_and_missing_or_malformed_trait_producers_fail_closed() {
    let (mut e, mut stack) = position();
    e.state.players[0].battlefield.clear();
    assert!(e.resolve_damage_source_lki(&stack).is_err());
    stack
        .decisions
        .insert("sourceLkiDeparted".into(), json!(true));
    assert!(e.resolve_damage_source_lki(&stack).is_err());
    stack
        .decisions
        .insert(DAMAGE_CAPTURE.into(), json!({"version":1}));
    assert!(e.resolve_damage_source_lki(&stack).is_err());
}
#[test]
fn shield_and_regeneration_survivors_are_not_marked_as_departed() {
    for protection in ["shield", "regeneration"] {
        let (mut e, stack) = position();
        let id = stack.card.instance_id.clone();
        e.state.stack.push(stack.clone());
        if protection == "shield" {
            e.state.players[0].battlefield[0]
                .counters
                .insert("shield".into(), 1);
        } else {
            e.state
                .rule_modifiers
                .push(json!({"kind":"regenerationShield","cardInstanceId":id}));
        }
        e.destroy_permanent_collection_from_stack(vec![id.clone()], false, &stack)
            .unwrap();
        let pending = &e.state.stack[0];
        assert!(!super::super::source_has_departed(pending));
        assert!(e.permanent_position(&id).is_some());
        assert!(!e.resolve_damage_source_lki(pending).unwrap().last_known);
    }
}
#[test]
fn snapshot_preserves_effective_types_colors_and_infect_independent_of_later_modifiers() {
    let (mut e, stack) = position();
    let id = stack.card.instance_id.clone();
    let turn = e.state.turn_number;
    e.state.rule_modifiers.extend([json!({"kind":"temporaryCardType","objectId":id,"cardType":"Creature"}),json!({"kind":"temporaryColors","cardInstanceId":id,"colors":["G"],"expiresAfterTurn":turn}),json!({"kind":"temporaryKeywordUntilTurn","cardInstanceId":id,"keyword":"infect","expiresBeforeTurn":turn+1})]);
    e.state.stack.push(stack);
    e.return_permanent_to_hand(&id).unwrap();
    e.state.rule_modifiers.clear();
    let pending = e.state.stack.pop().unwrap();
    let resolved = e.resolve_damage_source_lki(&pending).unwrap();
    assert!(resolved.properties.is_creature);
    assert!(resolved.properties.infect);
    assert_eq!(resolved.properties.colors, BTreeSet::from(["G".into()]));
    e.execute_player_set_damage(&effect(), &pending, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    for p in &e.state.players[1..] {
        assert_eq!(p.life, 20);
        assert_eq!(p.counters.get("poison"), Some(&1));
    }
}

#[test]
fn destruction_freezes_source_traits_before_a_simultaneously_destroyed_grant_is_removed() {
    let (mut e, stack) = position();
    let source_id = stack.card.instance_id.clone();
    let mut grant = definition("grant-lifelink");
    grant.type_line = "Artifact".into();
    grant.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"grantKeyword","keyword":"lifelink","objects":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Enchantment"}}}]}),
    ];
    assert!(battlefield_static_rule_supported(&grant.rules[0]));
    let mut grant = card_instance("player-0", &grant, 891);
    e.prepare_battlefield_entry_identity(&mut grant);
    let grant_id = grant.instance_id.clone();
    e.state.players[0].battlefield.push(grant);
    e.state.stack.push(stack.clone());
    e.destroy_permanent_collection_from_stack(vec![grant_id, source_id], false, &stack)
        .unwrap();
    let pending = e.state.stack.pop().unwrap();
    assert!(
        e.resolve_damage_source_lki(&pending)
            .unwrap()
            .properties
            .lifelink
    );
    e.execute_player_set_damage(&effect(), &pending, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [23, 19, 19, 19]
    );
}
#[test]
fn a_departure_during_unframed_local_resolution_remains_explicitly_unsupported() {
    let (mut e, stack) = position();
    let id = stack.card.instance_id.clone();
    assert!(e.state.stack.is_empty());
    e.return_permanent_to_hand(&id).unwrap();
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn mismatched_captured_incarnation_is_not_accepted_by_matching_physical_id() {
    let (mut e, stack) = position();
    let id = stack.card.instance_id.clone();
    e.state.stack.push(stack);
    e.return_permanent_to_hand(&id).unwrap();
    let mut pending = e.state.stack.pop().unwrap();
    pending.decisions.get_mut(DAMAGE_CAPTURE).unwrap()["zoneIdentity"] =
        json!("battlefield:wrong-incarnation");
    assert!(e.resolve_damage_source_lki(&pending).is_err());
}
