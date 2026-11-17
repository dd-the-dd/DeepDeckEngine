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
        rules: Vec::new(),
    }
}
fn game() -> (GameEngine, String) {
    let mut e = GameEngine::new(
        GameSetup {
            players: (0..4)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        16405,
    )
    .unwrap();
    let mut card = card_instance("player-0", &definition("damage-source"), 90);
    e.prepare_battlefield_entry_identity(&mut card);
    let id = card.instance_id.clone();
    e.state.players[0].battlefield.push(card);
    (e, id)
}
fn opponents() -> Vec<String> {
    (1..4).map(|i| format!("player-{i}")).collect()
}
fn run(e: &mut GameEngine, source: &str, n: i32) {
    let b = e
        .prepare_player_set_damage(&opponents(), n, source, None)
        .unwrap();
    e.commit_player_set_damage(b).unwrap();
}
fn observer(e: &mut GameEngine, scope: &str) {
    let mut d = definition(scope);
    d.rules = vec![
        json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"damageDealtToOpponent","player":{"kind":"abilityController"},"aggregation":scope},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]}),
    ];
    assert!(triggered_rule_supported(&d.rules[0]));
    let mut card = card_instance("player-0", &d, 100 + e.state.players[0].battlefield.len());
    e.prepare_battlefield_entry_identity(&mut card);
    e.state.players[0].battlefield.push(card);
}
fn modifier(
    e: &mut GameEngine,
    owner: usize,
    id: &str,
    mut m: Value,
    attached: Option<&str>,
) -> String {
    let mut d = definition(id);
    if value_kind(&m) != Some("preventNothing") {
        m["player"] = json!({"kind":"controllerOf","object":{"kind":"self"}});
        if value_kind(&m) == Some("redirectPlayerDamageToAttachedPermanent") {
            m["attachment"] = json!({"kind":"self"});
        }
        d.rules = vec![
            json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[m]}),
        ];
        assert!(battlefield_static_rule_supported(&d.rules[0]));
    }
    let mut c = card_instance(
        &format!("player-{owner}"),
        &d,
        120 + e.state.players[owner].battlefield.len(),
    );
    c.attached_to = attached.map(str::to_owned);
    e.prepare_battlefield_entry_identity(&mut c);
    let id = c.instance_id.clone();
    e.state.players[owner].battlefield.push(c);
    id
}
#[test]
fn four_players_receive_one_shared_damage_event_and_three_records() {
    let (mut e, s) = game();
    let before = e.state.events.len();
    run(&mut e, &s, 1);
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 19, 19, 19]
    );
    let records = e.state.events[before..]
        .iter()
        .filter(|x| x.kind == "damageDealt")
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 3);
    let grouped = e.state.events[before..]
        .iter()
        .find(|x| x.kind == "damageDealtTogether")
        .unwrap();
    assert_eq!(grouped.detail["recipients"].as_array().unwrap().len(), 3);
    assert!(
        records
            .iter()
            .all(|r| r.detail["batchId"] == grouped.detail["batchId"])
    );
}
#[test]
fn one_or_more_queues_once_while_per_recipient_queues_three() {
    let (mut e, s) = game();
    observer(&mut e, "oneOrMore");
    observer(&mut e, "perRecipient");
    run(&mut e, &s, 2);
    assert_eq!(e.state.stack.len(), 4);
    let grouped = e
        .state
        .stack
        .iter()
        .find(|x| x.card.definition.id == "oneOrMore")
        .unwrap();
    assert_eq!(grouped.decisions["damageAmount"], json!(6));
    assert_eq!(
        grouped.decisions["damagedPlayerIds"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(!grouped.decisions.contains_key("triggeringPlayerId"));
    assert_eq!(
        e.state
            .stack
            .iter()
            .filter(|x| x.card.definition.id == "perRecipient")
            .count(),
        3
    );
}
#[test]
fn fully_prevented_player_is_excluded_from_the_group() {
    let (mut e, s) = game();
    observer(&mut e, "oneOrMore");
    modifier(
        &mut e,
        2,
        "protection",
        json!({"kind":"preventPlayerDamage"}),
        None,
    );
    run(&mut e, &s, 2);
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 18, 20, 18]
    );
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(e.state.stack[0].decisions["damageAmount"], json!(4));
    assert_eq!(
        e.state.stack[0].decisions["damagedPlayerIds"],
        json!(["player-1", "player-3"])
    );
}
#[test]
fn fixed_prevention_reuses_shared_modifiers_for_each_actual_recipient() {
    let (mut e, s) = game();
    modifier(
        &mut e,
        2,
        "reduction",
        json!({"kind":"preventOpponentDamageAmount","amount":1}),
        None,
    );
    run(&mut e, &s, 3);
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 17, 18, 17]
    );
}
#[test]
fn lifelink_is_one_gain_for_the_total_damage() {
    let (mut e, s) = game();
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    run(&mut e, &s, 2);
    assert_eq!(e.state.players[0].life, 26);
    let gains = e
        .state
        .events
        .iter()
        .filter(|x| x.kind == "lifeGained")
        .collect::<Vec<_>>();
    assert_eq!(gains.len(), 1);
    assert_eq!(gains[0].detail["amount"], json!(6));
}
#[test]
fn lifelink_and_damage_to_its_controller_commit_one_combined_life_delta() {
    let (mut e, s) = game();
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    let all = (0..4).map(|i| format!("player-{i}")).collect::<Vec<_>>();
    let b = e.prepare_player_set_damage(&all, 2, &s, None).unwrap();
    e.commit_player_set_damage(b).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [26, 18, 18, 18]
    );
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|x| x.kind == "lifeGained")
            .count(),
        1
    );
}
#[test]
fn infect_changes_poison_not_life_and_still_has_damage_records() {
    let (mut e, s) = game();
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"infect"}}),
    );
    run(&mut e, &s, 2);
    for p in &e.state.players[1..] {
        assert_eq!(p.life, 20);
        assert_eq!(p.counters.get("poison"), Some(&2));
    }
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|x| x.kind == "damageDealt")
            .count(),
        3
    );
}
#[test]
fn converging_redirections_are_capped_before_damage_or_shield_consumption() {
    let (mut e, s) = game();
    let protected = modifier(
        &mut e,
        0,
        "protected",
        json!({"kind":"preventNothing"}),
        None,
    );
    e.state.players[0]
        .battlefield
        .last_mut()
        .unwrap()
        .counters
        .insert("shield".into(), 2);
    modifier(
        &mut e,
        1,
        "redirect-a",
        json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
        Some(&protected),
    );
    modifier(
        &mut e,
        2,
        "redirect-b",
        json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
        Some(&protected),
    );
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .prepare_player_set_damage(&opponents(), 2, &s, None)
        .err()
        .expect("replacement ordering is not implemented");
    assert!(
        error
            .to_string()
            .contains("redirection requires ordered replacement choices")
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn unsupported_unprevented_redirect_is_rejected_before_any_damage_or_event() {
    let (mut e, s) = game();
    let protected = modifier(
        &mut e,
        0,
        "protected",
        json!({"kind":"preventNothing"}),
        None,
    );
    modifier(
        &mut e,
        2,
        "redirect",
        json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
        Some(&protected),
    );
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 2, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn stale_player_plan_fails_atomically_before_other_players_change() {
    let (mut e, s) = game();
    let b = e
        .prepare_player_set_damage(&opponents(), 2, &s, None)
        .unwrap();
    e.state.players[2].life = 25;
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(e.commit_player_set_damage(b).is_err());
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn zero_damage_is_not_an_event_or_trigger() {
    let (mut e, s) = game();
    observer(&mut e, "oneOrMore");
    let before = e.state.events.len();
    run(&mut e, &s, 0);
    assert_eq!(e.state.events.len(), before);
    assert!(e.state.stack.is_empty());
}
#[test]
fn scalar_and_batch_share_individual_bonus_or_prevention_outcomes() {
    for bonus in [true, false] {
        let (mut batch, source) = game();
        if bonus {
            modifier(
                &mut batch,
                0,
                "bonus",
                json!({"kind":"increaseControlledNoncombatDamage","amount":1}),
                None,
            );
        } else {
            modifier(
                &mut batch,
                2,
                "reduction",
                json!({"kind":"preventOpponentDamageAmount","amount":1}),
                None,
            );
        }
        let mut scalar = batch.clone();
        run(&mut batch, &source, 2);
        for player in opponents() {
            scalar
                .deal_damage_amount_with_kind_and_origin(
                    TargetRef::Player { player_id: player },
                    2,
                    &source,
                    false,
                    QuantityOrigin::Effect,
                    None,
                )
                .unwrap();
        }
        assert_eq!(
            batch
                .state
                .players
                .iter()
                .map(|p| p.life)
                .collect::<Vec<_>>(),
            scalar
                .state
                .players
                .iter()
                .map(|p| p.life)
                .collect::<Vec<_>>()
        );
    }
}

fn stack_for(e: &GameEngine) -> StackObject {
    StackObject {
        id: "damage-instruction".into(),
        controller: "player-0".into(),
        card: e.state.players[0].battlefield[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}
fn effect() -> Value {
    json!({"kind":"dealDamage","source":{"kind":"self"},"amount":{"kind":"integer","value":1},"recipient":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}}})
}
#[test]
fn generic_ir_resolves_recipients_by_ability_controller_and_lifelink_by_source_controller() {
    let (mut e, _) = game();
    let stack = stack_for(&e);
    e.state.players[0].battlefield[0].controller = "player-1".into();
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    assert!(
        e.execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
    );
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 22, 19, 19]
    );
}
#[test]
fn missing_numeric_producer_errors_instead_of_substituting_zero() {
    let (mut e, _) = game();
    let stack = stack_for(&e);
    let mut effect = effect();
    effect["amount"] = json!({"kind":"decisionResult","decisionId":"missing"});
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.execute_player_set_damage(&effect, &stack, &BTreeMap::new(), &BTreeMap::new())
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn blinked_source_requires_effective_source_lki_before_damage() {
    let (mut e, _) = game();
    let stack = stack_for(&e);
    let mut card = e.state.players[0].battlefield.remove(0);
    e.prepare_battlefield_entry_identity(&mut card);
    e.state.players[0].battlefield.push(card);
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn resolving_spell_source_uses_interpreter_stack_object_after_pop() {
    let (mut e, _) = game();
    let mut stack = stack_for(&e);
    stack.ability_kind = None;
    assert!(stack_object_is_spell(&stack));
    stack.card.definition.type_line = "Instant".into();
    e.state.players[0].battlefield.clear();
    assert!(e.state.stack.is_empty());
    assert!(
        e.execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
    );
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 19, 19, 19]
    );
}
#[test]
fn unknown_aggregation_and_competing_redirections_fail_before_changes() {
    let (mut e, s) = game();
    observer(&mut e, "oneOrMore");
    e.state.players[0].battlefield[1].definition.rules[0]["event"]["aggregation"] =
        json!("arbitrary");
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 1, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    e.state.players[0].battlefield.pop();
    let p = modifier(
        &mut e,
        0,
        "protected",
        json!({"kind":"preventNothing"}),
        None,
    );
    modifier(
        &mut e,
        1,
        "redirect-a",
        json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
        Some(&p),
    );
    modifier(
        &mut e,
        1,
        "redirect-b",
        json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
        Some(&p),
    );
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 1, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn intervening_if_observes_simultaneous_lifelink_history_before_damage_triggers_queue() {
    let (mut e, s) = game();
    observer(&mut e, "oneOrMore");
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    e.state.players[0].battlefield[1].definition.rules[0]["condition"] = json!({"kind":"compare","operator":">=","left":{"kind":"lifeGainedThisTurn","player":{"kind":"controllerOf","object":{"kind":"self"}}},"right":{"kind":"integer","value":1}});
    assert!(triggered_rule_supported(
        &e.state.players[0].battlefield[1].definition.rules[0]
    ));
    run(&mut e, &s, 2);
    assert_eq!(e.state.players[0].life, 26);
    assert_eq!(e.state.stack.len(), 1);
    let gain = e
        .state
        .events
        .iter()
        .find(|event| event.kind == "lifeGained")
        .unwrap();
    assert_eq!(gain.detail["amount"], json!(6));
    let queued = e
        .state
        .events
        .iter()
        .find(|event| event.kind == "triggeredAbilityPutOnStack")
        .unwrap();
    assert!(gain.sequence < queued.sequence);
}
fn prevent_other_recipients(e: &mut GameEngine) {
    for owner in [2, 3] {
        modifier(
            e,
            owner,
            &format!("prevent-{owner}"),
            json!({"kind":"preventPlayerDamage"}),
            None,
        );
    }
}
#[test]
fn damage_bonus_overflow_errors_before_any_state_or_event_change() {
    let (mut e, s) = game();
    prevent_other_recipients(&mut e);
    modifier(
        &mut e,
        0,
        "max-bonus",
        json!({"kind":"increaseControlledNoncombatDamage","amount":i32::MAX}),
        None,
    );
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 1, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn damage_multiplier_product_and_amount_overflow_are_checked_before_changes() {
    for factors in [vec![i32::MAX, 2], vec![2]] {
        let (mut e, s) = game();
        prevent_other_recipients(&mut e);
        for factor in &factors {
            e.state
                .rule_modifiers
                .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":factor}));
        }
        let amount = if factors.len() == 1 { i32::MAX } else { 1 };
        let before = serde_json::to_value(&e.state).unwrap();
        assert!(
            e.prepare_player_set_damage(&opponents(), amount, &s, None)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    }
}
#[test]
fn life_gain_multiplier_overflow_errors_before_damage_changes() {
    let (mut e, s) = game();
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    for i in 0..31 {
        modifier(
            &mut e,
            0,
            &format!("gain-double-{i}"),
            json!({"kind":"multiplyLifeGain","factor":2}),
            None,
        );
    }
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 1, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn prevention_sum_overflow_errors_instead_of_wrapping_or_saturating() {
    let (mut e, s) = game();
    prevent_other_recipients(&mut e);
    for i in 0..2 {
        modifier(
            &mut e,
            1,
            &format!("prevent-max-{i}"),
            json!({"kind":"preventOpponentDamageAmount","amount":i32::MAX}),
            None,
        );
    }
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.prepare_player_set_damage(&opponents(), 2, &s, None)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
#[test]
fn simultaneous_lifelink_can_offset_own_damage_without_false_intermediate_overflow() {
    let (mut e, s) = game();
    e.state.players[0].life = i32::MAX;
    e.state.players[0].battlefield[0].definition.rules.push(
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"lifelink"}}),
    );
    let b = e
        .prepare_player_set_damage(&["player-0".into()], 1, &s, None)
        .unwrap();
    e.commit_player_set_damage(b).unwrap();
    assert_eq!(e.state.players[0].life, i32::MAX);
    assert_eq!(
        e.state
            .events
            .iter()
            .find(|x| x.kind == "lifeGained")
            .unwrap()
            .detail["amount"],
        json!(1)
    );
}

#[test]
fn opponent_taking_damage_is_observed_even_when_that_opponent_controls_the_source() {
    let (mut e, _) = game();
    e.state.players[0].battlefield.clear();
    let mut damage_source = card_instance("player-1", &definition("opponent-damage-source"), 810);
    e.prepare_battlefield_entry_identity(&mut damage_source);
    let source = damage_source.instance_id.clone();
    e.state.players[1].battlefield.push(damage_source);
    observer(&mut e, "perRecipient");
    e.state.players[0].battlefield[0].definition.rules[0]["event"]["kind"] =
        json!("opponentDealtDamage");
    assert!(triggered_rule_supported(
        &e.state.players[0].battlefield[0].definition.rules[0]
    ));
    let all = (0..4).map(|i| format!("player-{i}")).collect::<Vec<_>>();
    let batch = e.prepare_player_set_damage(&all, 1, &source, None).unwrap();
    e.commit_player_set_damage(batch).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [19, 19, 19, 19]
    );
    assert_eq!(e.state.stack.len(), 3);
    let observed = e
        .state
        .stack
        .iter()
        .map(|object| object.decisions["triggeringPlayerId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        observed,
        BTreeSet::from(["player-1", "player-2", "player-3"])
    );
}

#[test]
fn redirection_with_bonus_or_player_prevention_is_rejected_without_consuming_shields() {
    for kind in ["bonus", "prevention", "multiplier", "plain"] {
        let (mut e, source) = game();
        let protected = modifier(
            &mut e,
            0,
            "protected",
            json!({"kind":"preventNothing"}),
            None,
        );
        e.state.players[0]
            .battlefield
            .last_mut()
            .unwrap()
            .counters
            .insert("shield".into(), 2);
        modifier(
            &mut e,
            1,
            "redirect",
            json!({"kind":"redirectPlayerDamageToAttachedPermanent"}),
            Some(&protected),
        );
        match kind {
            "bonus" => {
                modifier(
                    &mut e,
                    0,
                    "bonus",
                    json!({"kind":"increaseControlledNoncombatDamage","amount":1}),
                    None,
                );
            }
            "prevention" => {
                modifier(
                    &mut e,
                    1,
                    "prevent",
                    json!({"kind":"preventPlayerDamage"}),
                    None,
                );
            }
            "multiplier" => e
                .state
                .rule_modifiers
                .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":2})),
            _ => {}
        }
        let before = serde_json::to_value(&e.state).unwrap();
        let error = e
            .prepare_player_set_damage(&opponents(), 1, &source, None)
            .err()
            .expect(kind);
        assert!(
            error
                .to_string()
                .contains("redirection requires ordered replacement choices")
        );
        assert_eq!(serde_json::to_value(&e.state).unwrap(), before, "{kind}");
    }
}
#[test]
fn unproved_replacement_class_combinations_fail_atomically() {
    for kind in [
        "add-multiply",
        "add-fixed",
        "multiply-fixed",
        "add-full",
        "multiply-full",
        "fixed-full",
        "full-full",
        "counter-add-multiply",
    ] {
        let (mut e, source) = game();
        if kind.starts_with("add-") {
            modifier(
                &mut e,
                0,
                "bonus",
                json!({"kind":"increaseControlledNoncombatDamage","amount":2}),
                None,
            );
        }
        if kind.contains("multiply") {
            e.state
                .rule_modifiers
                .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":2}));
        }
        if kind.contains("fixed") {
            modifier(
                &mut e,
                1,
                "fixed",
                json!({"kind":"preventOpponentDamageAmount","amount":2}),
                None,
            );
        }
        if kind.contains("full") {
            modifier(
                &mut e,
                1,
                "full",
                json!({"kind":"preventPlayerDamage"}),
                None,
            );
        }
        if kind == "full-full" {
            e.state
                .rule_modifiers
                .push(json!({"kind":"protectionFromEverything","playerId":"player-1"}));
        }
        if kind == "counter-add-multiply" {
            modifier(
                &mut e,
                0,
                "counter-bonus",
                json!({"kind":"increaseDamageBySourceCounter","counterSource":{"kind":"self"},"counter":"charge"}),
                None,
            );
            e.state.players[0]
                .battlefield
                .last_mut()
                .unwrap()
                .counters
                .insert("charge".into(), 1);
        }
        let before = serde_json::to_value(&e.state).unwrap();
        let error = e
            .prepare_player_set_damage(&opponents(), 1, &source, None)
            .err()
            .expect(kind);
        assert!(
            error
                .to_string()
                .contains("competing replacements requiring affected-player ordering choices"),
            "{kind}: {error}"
        );
        assert_eq!(serde_json::to_value(&e.state).unwrap(), before, "{kind}");
    }
}
#[test]
fn commuting_pure_replacement_classes_keep_their_proved_totals() {
    for kind in ["additions", "multipliers", "fixed-preventions"] {
        let (mut e, source) = game();
        match kind {
            "additions" => {
                for n in [2, 3] {
                    modifier(
                        &mut e,
                        0,
                        &format!("bonus-{n}"),
                        json!({"kind":"increaseControlledNoncombatDamage","amount":n}),
                        None,
                    );
                }
            }
            "multipliers" => {
                for n in [2, 3] {
                    e.state
                        .rule_modifiers
                        .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":n}));
                }
            }
            _ => {
                for n in [1, 2] {
                    modifier(
                        &mut e,
                        1,
                        &format!("prevent-{n}"),
                        json!({"kind":"preventOpponentDamageAmount","amount":n}),
                        None,
                    );
                }
            }
        }
        let initial = if kind == "fixed-preventions" { 5 } else { 1 };
        run(&mut e, &source, initial);
        let expected = if kind == "fixed-preventions" {
            vec![20, 18, 15, 15]
        } else {
            vec![20, 14, 14, 14]
        };
        assert_eq!(
            e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
            expected,
            "{kind}"
        );
    }
}
#[test]
fn source_linked_prevention_competing_with_numeric_modification_is_capped() {
    let (mut e, source) = game();
    let linked = modifier(
        &mut e,
        1,
        "linked-source",
        json!({"kind":"preventNothing"}),
        None,
    );
    modifier(
        &mut e,
        0,
        "bonus",
        json!({"kind":"increaseControlledNoncombatDamage","amount":1}),
        None,
    );
    e.state.rule_modifiers.push(json!({"kind":"linkedDamagePrevention","damagingCardInstanceId":source,"sourceCardInstanceId":linked}));
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .prepare_player_set_damage(&opponents(), 1, &source, None)
        .err()
        .expect("replacement ordering cap");
    assert!(error.to_string().contains("competing replacements"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn unrelated_controller_and_unselected_recipient_modifiers_do_not_create_false_competition() {
    let (mut e, source) = game();
    modifier(
        &mut e,
        2,
        "other-controllers-bonus",
        json!({"kind":"increaseControlledNoncombatDamage","amount":1}),
        None,
    );
    e.state
        .rule_modifiers
        .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":2}));
    run(&mut e, &source, 1);
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 18, 18, 18]
    );
    let (mut e, source) = game();
    modifier(
        &mut e,
        0,
        "bonus",
        json!({"kind":"increaseControlledNoncombatDamage","amount":1}),
        None,
    );
    modifier(
        &mut e,
        3,
        "unselected-prevention",
        json!({"kind":"preventOpponentDamageAmount","amount":1}),
        None,
    );
    let batch = e
        .prepare_player_set_damage(&["player-1".into()], 1, &source, None)
        .unwrap();
    e.commit_player_set_damage(batch).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        [20, 18, 20, 20]
    );
}

#[test]
fn negative_counter_bonus_and_zero_negative_fractional_factors_are_not_commutative_classes() {
    for factor in [
        json!(0),
        json!(-2),
        json!(0.5),
        json!({"kind":"integer","value":1.5}),
        json!({"kind":"divide","left":{"kind":"integer","value":1},"right":{"kind":"integer","value":2}}),
    ] {
        let (mut e, source) = game();
        modifier(
            &mut e,
            1,
            "full-prevention",
            json!({"kind":"preventPlayerDamage"}),
            None,
        );
        e.state
            .rule_modifiers
            .push(json!({"kind":"damageMultiplier","playerId":"player-0","factor":factor}));
        let before = serde_json::to_value(&e.state).unwrap();
        let error = e
            .prepare_player_set_damage(&opponents(), 1, &source, None)
            .err()
            .expect("unproved factor domain");
        assert!(error.to_string().contains("integer factor of at least one"));
        assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    }
    let (mut e, source) = game();
    modifier(
        &mut e,
        0,
        "counter-bonus",
        json!({"kind":"increaseDamageBySourceCounter","counterSource":{"kind":"self"},"counter":"charge"}),
        None,
    );
    e.state.players[0]
        .battlefield
        .last_mut()
        .unwrap()
        .counters
        .insert("charge".into(), -1);
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .prepare_player_set_damage(&opponents(), 1, &source, None)
        .err()
        .expect("negative counter quantity");
    assert!(error.to_string().contains("negative counter quantity"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
