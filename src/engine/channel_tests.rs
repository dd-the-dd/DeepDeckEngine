use super::*;

fn action_flag(action: &LegalAction, name: &str) -> bool {
    action.decisions.get(name).and_then(Value::as_bool) == Some(true)
}

fn channel_land(name: &str, color: &str, cost: &str, effect: &str) -> CardInstance {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Legendary Land".to_string(),
        mana_cost: None,
        oracle_text: Some(format!(
            "{{T}}: Add {{{color}}}.\nChannel \u{2014} {cost}, Discard this card: {effect} This ability costs {{1}} less to activate for each legendary creature you control."
        )),
        layout: None,
        faces: Vec::new(),
    });
    let mut definition = test_definition(name, "Legendary Land");
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    assert!(
        definition
            .rules
            .iter()
            .any(|rule| { activated_rule_supported(rule) && rule["activationZone"] == "hand" }),
        "Channel must compile as an executable hand activation: {name}: {:?}",
        definition.rules
    );
    test_instance("channel-land", definition, "player-0")
}

fn channel_lands() -> Vec<CardInstance> {
    vec![
        channel_land(
            "Boseiju, Who Endures",
            "G",
            "{1}{G}",
            "Destroy target artifact, enchantment, or nonbasic land an opponent controls. That player may search their library for a land card with a basic land type, put it onto the battlefield, then shuffle.",
        ),
        channel_land(
            "Otawara, Soaring City",
            "U",
            "{3}{U}",
            "Return target artifact, creature, enchantment, or planeswalker to its owner's hand.",
        ),
        channel_land(
            "Eiganjo, Seat of the Empire",
            "W",
            "{2}{W}",
            "It deals 4 damage to target attacking or blocking creature.",
        ),
        channel_land(
            "Sokenzan, Crucible of Defiance",
            "R",
            "{3}{R}",
            "Create two 1/1 colorless Spirit creature tokens. They gain haste until end of turn.",
        ),
        channel_land(
            "Takenuma, Abandoned Mire",
            "B",
            "{3}{B}",
            "Mill three cards, then return a creature or planeswalker card from your graveyard to your hand.",
        ),
    ]
}

fn channel_engine() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand.clear();
    engine.state.players[0].battlefield.clear();
    for symbol in ["W", "U", "B", "R", "G", "C", "C", "C", "C"] {
        engine.state.players[0].mana_pool.push(FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        });
    }
    let mut target = test_instance(
        "legal-target",
        test_definition("legal-target", "Artifact Creature"),
        "player-1",
    );
    target.flags.insert("attacking".to_string(), true);
    engine.state.players[1].battlefield.push(target);
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "legal-target".to_string(),
        defender: TargetRef::Player {
            player_id: "player-0".to_string(),
        },
        defending_player_id: "player-0".to_string(),
    });
    engine
}

#[test]
fn channel_all_five_lands_are_hand_only_but_keep_battlefield_mana() {
    for source in channel_lands() {
        for zone in ["battlefield", "graveyard", "exile", "hand"] {
            let mut engine = channel_engine();
            match zone {
                "battlefield" => engine.state.players[0].battlefield.push(source.clone()),
                "graveyard" => engine.state.players[0].graveyard.push(source.clone()),
                "exile" => engine.state.players[0].exile.push(source.clone()),
                "hand" => engine.state.players[0].hand.push(source.clone()),
                _ => unreachable!(),
            }
            let actions = engine.legal_priority_actions(0);
            let own_activations = actions
                .iter()
                .filter(|action| {
                    action.kind == ActionKind::ActivateAbility
                        && action.card_instance_id.as_deref() == Some("channel-land")
                })
                .collect::<Vec<_>>();
            assert_eq!(
                own_activations
                    .iter()
                    .any(|action| action_flag(action, "offBattlefieldActivation")),
                zone == "hand",
                "{} from {zone}: {own_activations:?}",
                source.definition.name
            );
            assert_eq!(
                own_activations
                    .iter()
                    .any(|action| action_flag(action, "manaAbility")),
                zone == "battlefield",
                "{} normal mana from {zone}",
                source.definition.name
            );
            assert!(
                own_activations.iter().all(|action| {
                    action_flag(action, "manaAbility")
                        || action_flag(action, "offBattlefieldActivation")
                }),
                "Channel must never appear as an ordinary battlefield activation: {} from {zone}",
                source.definition.name
            );
        }
    }
}

#[test]
fn channel_boseiju_discards_as_cost_and_resolves_without_self_funding() {
    let mut engine = channel_engine();
    let source = channel_lands().remove(0);
    engine.state.players[0].hand.push(source);
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("channel-land")
                && action_flag(action, "offBattlefieldActivation")
        })
        .expect("Boseiju in hand can channel with an opponent's artifact as target");
    assert!(
        !action
            .payment_sources
            .iter()
            .any(|source| source == "channel-land")
    );
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(31))
        .unwrap();
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .all(|card| card.instance_id != "channel-land")
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "channel-land")
    );
    assert_eq!(
        engine.state.stack.len(),
        1,
        "Channel uses the stack, not a spell cast"
    );
    assert_eq!(
        engine.state.stack[0].ability_kind.as_deref(),
        Some("activatedAbility")
    );
    engine.resolve_top_stack(&mut RandomAi::seeded(31)).unwrap();
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .all(|card| card.instance_id != "legal-target")
    );
}

#[test]
fn channel_invalid_battlefield_action_is_rejected_before_payment_or_stack_mutation() {
    let mut engine = channel_engine();
    engine.state.players[0].hand.push(channel_lands().remove(0));
    let mut action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("channel-land")
                && action_flag(action, "offBattlefieldActivation")
        })
        .unwrap();
    action.decisions.remove("offBattlefieldActivation");
    action.decisions.remove("activationSourceZone");
    let card = engine.state.players[0].hand.pop().unwrap();
    engine.state.players[0].battlefield.push(card);
    let before = serde_json::to_value(&engine.state).unwrap();
    let error = engine
        .apply_priority_action(&action, &mut RandomAi::seeded(32))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot be activated from the battlefield")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn channel_stale_hand_action_is_rejected_before_payment() {
    let mut engine = channel_engine();
    engine.state.players[0].hand.push(channel_lands().remove(0));
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("channel-land")
                && action_flag(action, "offBattlefieldActivation")
        })
        .unwrap();
    let card = engine.state.players[0].hand.pop().unwrap();
    engine.state.players[0].exile.push(card);
    let before = serde_json::to_value(&engine.state).unwrap();
    assert!(
        engine
            .apply_priority_action(&action, &mut RandomAi::seeded(33))
            .is_err()
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn channel_requires_a_legal_target_and_payable_mana() {
    let mut engine = channel_engine();
    engine.state.players[0].hand.push(channel_lands().remove(0));
    engine.state.players[1].battlefield.clear();
    assert!(
        !engine
            .legal_priority_actions(0)
            .iter()
            .any(|action| action_flag(action, "offBattlefieldActivation"))
    );
    engine.state.players[1].battlefield.push(test_instance(
        "target",
        test_definition("target", "Artifact"),
        "player-1",
    ));
    engine.state.players[0].mana_pool.clear();
    assert!(
        !engine
            .legal_priority_actions(0)
            .iter()
            .any(|action| action_flag(action, "offBattlefieldActivation"))
    );
}

#[test]
fn channel_legendary_reduction_preserves_colored_mana_floor_for_all_five_lands() {
    for (source, color) in channel_lands().into_iter().zip(["G", "U", "W", "R", "B"]) {
        let mut engine = channel_engine();
        engine.state.players[0].mana_pool = vec![FloatingMana {
            symbol: color.to_string(),
            spend_restriction: None,
        }];
        for index in 0..5 {
            let id = format!("legend-{index}");
            engine.state.players[0].battlefield.push(test_instance(
                &id,
                test_definition(&id, "Legendary Creature"),
                "player-0",
            ));
        }
        engine.state.players[0].hand.push(source.clone());
        assert!(
            engine.legal_priority_actions(0).iter().any(|action| {
                action.card_instance_id.as_deref() == Some("channel-land")
                    && action_flag(action, "offBattlefieldActivation")
            }),
            "legendary creatures reduce only the generic cost: {}",
            source.definition.name
        );
        engine.state.players[0].mana_pool[0].symbol = "C".to_string();
        assert!(
            !engine.legal_priority_actions(0).iter().any(|action| {
                action.card_instance_id.as_deref() == Some("channel-land")
                    && action_flag(action, "offBattlefieldActivation")
            }),
            "colored mana remains required even with five legends: {}",
            source.definition.name
        );
    }
}

#[test]
fn channel_zone_guard_defaults_to_battlefield_and_rejects_malformed_self_discard() {
    let mut rule =
        json!({"kind":"activatedAbility", "costs":[{"kind":"payMana", "manaCost":"{1}"}]});
    assert!(activated_rule_usable_from_zone(&rule, "battlefield"));
    assert!(!activated_rule_usable_from_zone(&rule, "hand"));
    rule["activationZone"] = json!("graveyard");
    assert!(activated_rule_usable_from_zone(&rule, "graveyard"));
    assert!(!activated_rule_usable_from_zone(&rule, "battlefield"));
    rule["activationZone"] = json!(["hand", "battlefield"]);
    assert!(!activated_rule_usable_from_zone(&rule, "battlefield"));
    rule.as_object_mut().unwrap().remove("activationZone");
    rule["costs"] = json!([{"kind":"discardCard", "card":{"kind":"self"}}]);
    assert!(!activated_rule_usable_from_zone(&rule, "battlefield"));
    rule["activationZone"] = json!("hand");
    assert!(activated_rule_usable_from_zone(&rule, "hand"));
}

#[test]
fn channel_nonland_creature_also_requires_hand_and_discards_before_damage() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Twinshot Sniper".to_string(),
        type_line: "Artifact Creature - Goblin Archer".to_string(),
        mana_cost: Some("{3}{R}".to_string()),
        oracle_text: Some(
            "Channel \u{2014} {1}{R}, Discard this card: It deals 2 damage to any target."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    let mut definition = test_definition("twinshot", "Artifact Creature - Goblin Archer");
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    assert!(definition.rules.iter().any(activated_rule_supported));
    let source = test_instance("twinshot", definition, "player-0");
    let mut engine = channel_engine();
    engine.state.players[0].battlefield.push(source.clone());
    assert!(!engine.legal_priority_actions(0).iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some("twinshot")
    }));
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].hand.push(source);
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("twinshot")
                && action.kind == ActionKind::ActivateAbility
                && action.targets.values().any(|target| {
                    target
                        == &TargetRef::Player {
                            player_id: "player-1".to_string(),
                        }
                })
        })
        .expect("a nonland Channel card can damage an opponent from hand");
    let life = engine.state.players[1].life;
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(34))
        .unwrap();
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "twinshot")
    );
    assert_eq!(
        engine.state.players[1].life, life,
        "discard happens before the ability resolves"
    );
    engine.resolve_top_stack(&mut RandomAi::seeded(34)).unwrap();
    assert_eq!(engine.state.players[1].life, life - 2);
}
