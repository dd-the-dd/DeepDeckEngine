use super::*;
use serde_json::json;

#[derive(Default)]
struct First {
    players: Vec<String>,
    expected_battlefield: Option<usize>,
}
impl DecisionProvider for First {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(expected) = self.expected_battlefield {
            assert_eq!(
                state
                    .players
                    .iter()
                    .map(|p| p.battlefield.len())
                    .sum::<usize>(),
                expected,
                "all players finish independent choices before any sacrifice"
            );
        }
        self.players.push(request.player_id.clone());
        Ok(0)
    }
}
fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}
fn position(players: usize) -> (GameEngine, StackObject) {
    let e = GameEngine::new(
        GameSetup {
            players: (0..players)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1894,
    )
    .unwrap();
    let source = card_instance("player-0", &definition("source", "Sorcery"), 9999);
    (e, stack(&source, "instruction"))
}
fn stack(source: &CardInstance, id: &str) -> StackObject {
    StackObject {
        id: id.to_string(),
        controller: source.controller.clone(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}
fn add(
    e: &mut GameEngine,
    owner: usize,
    controller: usize,
    id: &str,
    type_line: &str,
    rules: Vec<Value>,
) -> CardInstance {
    let index = e
        .state
        .players
        .iter()
        .map(|p| {
            p.library.len() + p.hand.len() + p.battlefield.len() + p.graveyard.len() + p.exile.len()
        })
        .sum::<usize>()
        + 1000;
    let mut card = card_instance(
        &format!("player-{owner}"),
        &definition(id, type_line),
        index,
    );
    card.controller = format!("player-{controller}");
    card.definition.rules = rules;
    card.printed_definition = Some(card.definition.clone());
    e.state.players[controller].battlefield.push(card);
    e.admit_explicit_test_fixture_objects()
        .expect("fixture construction admission before references or snapshots");
    e.state.players[controller]
        .battlefield
        .last()
        .unwrap()
        .clone()
}
fn count(e: &GameEngine, kind: &str) -> usize {
    e.state.events.iter().filter(|ev| ev.kind == kind).count()
}
fn observer(event: Value) -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":event,
    "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]})
}
fn static_rule(modifier: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
    "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[modifier]})
}
fn protection() -> Value {
    static_rule(
        json!({"kind":"preventOpponentForcedSacrifice","player":{"kind":"controllerOf","object":{"kind":"self"}}}),
    )
}
fn program() -> Value {
    let player = json!({"kind":"boundValue","id":"loop:player"});
    json!([
        {"kind":"forEachPlayer","players":{"kind":"opponentsOf","player":{"kind":"abilityController"}},"bind":"loop:player",
         "effects":[
           {"kind":"bind","id":"candidates","value":{"kind":"eachPermanent","player":player,"where":{"kind":"not","operand":{"kind":"cardTypeContains","value":"Land"}}}},
           {"kind":"chooseCards","id":"artifact","player":player,"from":{"kind":"boundObjects","binding":"candidates"},"where":{"kind":"cardTypeContains","value":"Artifact"},"count":{"kind":"integer","value":1}},
           {"kind":"chooseCards","id":"creature","player":player,"from":{"kind":"setDifference","left":{"kind":"boundObjects","binding":"candidates"},"right":{"kind":"decisionResult","decisionId":"artifact"}},"where":{"kind":"cardTypeContains","value":"Creature"},"count":{"kind":"integer","value":2}}
         ],"collect":{"bind":"allRemainders","objects":{"kind":"setDifference","left":{"kind":"setDifference","left":{"kind":"boundObjects","binding":"candidates"},"right":{"kind":"decisionResult","decisionId":"artifact"}},"right":{"kind":"decisionResult","decisionId":"creature"}}}},
        {"kind":"sacrificePermanents","player":{"kind":"eachPlayer"},"objects":{"kind":"boundObjects","binding":"allRemainders"}}
    ])
}

#[test]
fn simultaneous_death_and_sacrifice_observers_see_each_other_even_when_both_leave() {
    let (mut e, source) = position(2);
    let rules = vec![
        observer(
            json!({"kind":"permanentDied","where":{"kind":"cardTypeContains","value":"Creature"},"excludeSource":true,"player":{"kind":"controllerOf","object":{"kind":"self"}}}),
        ),
        observer(
            json!({"kind":"permanentSacrificed","where":{"kind":"cardTypeContains","value":"Creature"},"excludeSource":true,"player":{"kind":"controllerOf","object":{"kind":"self"}}}),
        ),
    ];
    assert!(rules.iter().all(rule_is_executable));
    let a = add(&mut e, 1, 1, "observer-a", "Creature", rules.clone());
    let b = add(&mut e, 1, 1, "observer-b", "Creature", rules);
    let actual = e
        .sacrifice_effect_collection(
            vec![a.instance_id.clone(), b.instance_id.clone()],
            &["player-1".to_string()],
            &source,
        )
        .unwrap();
    assert_eq!(actual, [a.instance_id, b.instance_id]);
    assert_eq!(e.state.players[1].graveyard.len(), 2);
    assert_eq!(e.state.stack.len(), 4);
    assert!(e.state.stack.iter().all(|s| s.controller == "player-1"));
    assert_eq!(count(&e, "permanentSacrificed"), 2);
}

#[test]
fn sacrifice_preserves_actual_controller_owner_and_effect_source_independently() {
    let (mut e, source) = position(2);
    let a = add(&mut e, 0, 1, "foreign-a", "Creature", vec![]);
    let b = add(&mut e, 1, 0, "foreign-b", "Creature", vec![]);
    e.sacrifice_effect_collection(
        vec![a.instance_id.clone(), b.instance_id.clone()],
        &["player-0".to_string(), "player-1".to_string()],
        &source,
    )
    .unwrap();
    assert_eq!(e.state.players[0].graveyard[0].instance_id, a.instance_id);
    assert_eq!(e.state.players[1].graveyard[0].instance_id, b.instance_id);
    let events = e
        .state
        .events
        .iter()
        .filter(|ev| ev.kind == "permanentSacrificed")
        .collect::<Vec<_>>();
    assert_eq!(events[0].player_id.as_deref(), Some("player-1"));
    assert_eq!(events[1].player_id.as_deref(), Some("player-0"));
    assert!(
        events
            .iter()
            .all(|ev| ev.detail["source"].as_str() == Some(source.card.instance_id.as_str()))
    );
}

#[test]
fn independent_choices_are_apnap_before_one_sacrifice_batch_with_or_without_protection() {
    for protected in [false, true] {
        let (mut e, source) = position(3);
        e.state.active_player = 2;
        for player in [1, 2] {
            for (name, kind) in [
                ("overlap", "Artifact Creature"),
                ("artifact-a", "Artifact"),
                ("artifact-b", "Artifact"),
                ("creature", "Creature"),
                ("enchantment", "Enchantment"),
            ] {
                add(
                    &mut e,
                    player,
                    player,
                    &format!("{name}-{player}"),
                    kind,
                    vec![],
                );
            }
        }
        if protected {
            add(&mut e, 1, 1, "protector", "Enchantment", vec![protection()]);
        }
        let mut provider = First {
            expected_battlefield: Some(if protected { 11 } else { 10 }),
            ..First::default()
        };
        let mut bindings = BTreeMap::new();
        let mut decisions = BTreeMap::new();
        let effects = program();
        assert!(effects.as_array().unwrap().iter().all(effect_supported));
        e.execute_effects(
            &effects,
            &source,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.players, ["player-2", "player-1"]);
        assert_eq!(e.state.players[2].battlefield.len(), 2);
        assert_eq!(e.state.players[2].graveyard.len(), 3);
        assert_eq!(
            e.state.players[1].battlefield.len(),
            if protected { 6 } else { 2 }
        );
        assert_eq!(
            e.state.players[1].graveyard.len(),
            if protected { 0 } else { 3 }
        );
        for player in [1, 2] {
            assert!(
                e.state.players[player]
                    .battlefield
                    .iter()
                    .any(|c| c.definition.id == format!("overlap-{player}"))
            );
            assert!(
                e.state.players[player]
                    .battlefield
                    .iter()
                    .any(|c| c.definition.id == format!("creature-{player}"))
            );
        }
        assert_eq!(
            count(&e, "permanentSacrificed"),
            if protected { 3 } else { 6 }
        );
    }
}

#[test]
fn destination_replacement_source_leaving_in_the_same_batch_still_exiles_all() {
    let (mut e, source) = position(2);
    let rule = static_rule(
        json!({"kind":"replaceGraveyardDestination","graveyardOwners":{"kind":"eachPlayer"},"where":null,"from":{"kind":"anyZone"},"to":{"kind":"exile"}}),
    );
    assert!(rule_is_executable(&rule));
    let replacement = add(
        &mut e,
        1,
        1,
        "replacement-source",
        "Enchantment",
        vec![rule],
    );
    let target = add(&mut e, 1, 1, "victim", "Creature", vec![]);
    let ids = e
        .sacrifice_effect_collection(
            vec![replacement.instance_id, target.instance_id],
            &["player-1".to_string()],
            &source,
        )
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(e.state.players[1].exile.len(), 2);
    assert!(e.state.players[1].graveyard.is_empty());
    assert_eq!(count(&e, "permanentSacrificed"), 2);
    assert_eq!(count(&e, "permanentDied"), 0);
}

#[test]
fn sacrifice_ignores_all_destruction_preventions() {
    let (mut e, source) = position(2);
    let mut victim = add(
        &mut e,
        1,
        1,
        "protected-creature",
        "Creature",
        vec![json!({"kind":"keywordAbility","ability":{"kind":"indestructible"}})],
    );
    victim.counters.insert("shield".to_string(), 1);
    e.state.players[1].battlefield[0] = victim.clone();
    let mut aura = add(
        &mut e,
        1,
        1,
        "umbra",
        "Enchantment - Aura",
        vec![json!({"kind":"keywordAbility","ability":{"kind":"umbraArmor"}})],
    );
    aura.attached_to = Some(victim.instance_id.clone());
    e.state.players[1].battlefield[1] = aura;
    e.state
        .rule_modifiers
        .push(json!({"kind":"regenerationShield","cardInstanceId":victim.instance_id}));
    let ids = e
        .sacrifice_permanent_for_effect(&victim.instance_id, &source)
        .unwrap();
    assert_eq!(ids, [victim.instance_id.clone()]);
    assert!(
        e.state.players[1]
            .graveyard
            .iter()
            .any(|c| c.instance_id == victim.instance_id)
    );
    assert_eq!(count(&e, "permanentSacrificed"), 1);
    assert_eq!(count(&e, "permanentRegenerated"), 0);
    assert_eq!(count(&e, "destructionPrevented"), 0);
    assert_eq!(count(&e, "destructionReplacedByUmbraArmor"), 0);
}

#[test]
fn pending_ability_lki_is_captured_before_lord_removal() {
    let (mut e, source) = position(2);
    let boost = static_rule(
        json!({"kind":"modifyPowerToughness","objects":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"}},"power":{"kind":"integer","value":2},"toughness":{"kind":"integer","value":0}}),
    );
    assert!(rule_is_executable(&boost));
    let lord = add(&mut e, 1, 1, "lord", "Enchantment", vec![boost]);
    let victim = add(&mut e, 1, 1, "victim", "Creature", vec![]);
    assert_eq!(e.current_power(&victim), 4);
    e.state.stack.push(stack(&victim, "pending-victim"));
    e.sacrifice_effect_collection(
        vec![lord.instance_id, victim.instance_id],
        &["player-1".to_string()],
        &source,
    )
    .unwrap();
    assert_eq!(e.state.stack[0].decisions["sourceLkiPower"], json!(4));
    assert_eq!(e.state.stack[0].decisions["sourceLkiDeparted"], json!(true));
}

#[test]
fn old_blinked_ability_keeps_its_first_lki_and_new_ability_captures_its_own_departure() {
    let (mut e, source) = position(2);
    let victim = add(&mut e, 1, 1, "victim", "Creature", vec![]);
    e.state.stack.push(stack(&victim, "old-ability"));
    e.sacrifice_permanent_for_effect(&victim.instance_id, &source)
        .unwrap();
    let mut bindings = BTreeMap::from([(
        "returning".to_string(),
        RuntimeBinding::Objects(vec![victim.instance_id.clone()]),
    )]);
    let movement = json!({"kind":"moveCards","cards":{"kind":"boundObjects","binding":"returning"},"from":{"kind":"graveyard","player":{"kind":"triggeringPlayer"}},"to":{"kind":"battlefield","player":{"kind":"triggeringPlayer"},"tapped":false}});
    assert!(effect_supported(&movement));
    let mut context = source.clone();
    context
        .decisions
        .insert("triggeringPlayerId".to_string(), json!("player-1"));
    e.execute_effect(
        &movement,
        None,
        &context,
        &mut bindings,
        &mut BTreeMap::new(),
        &mut First::default(),
    )
    .unwrap();
    let (player, index) = e.permanent_position(&victim.instance_id).unwrap();
    e.state.players[player].battlefield[index].power_modifier = 5;
    let returned = e.state.players[player].battlefield[index].clone();
    e.state.stack.push(stack(&returned, "new-ability"));
    e.sacrifice_permanent_for_effect(&returned.instance_id, &source)
        .unwrap();
    assert_eq!(
        e.state
            .stack
            .iter()
            .find(|s| s.id == "old-ability")
            .unwrap()
            .decisions["sourceLkiPower"],
        json!(2)
    );
    assert_eq!(
        e.state
            .stack
            .iter()
            .find(|s| s.id == "new-ability")
            .unwrap()
            .decisions["sourceLkiPower"],
        json!(7)
    );
}

#[test]
fn protected_candidate_is_not_frozen_but_an_actual_cost_can_sacrifice_it() {
    let (mut e, source) = position(2);
    add(&mut e, 1, 1, "protector", "Enchantment", vec![protection()]);
    let victim = add(&mut e, 1, 1, "cost-source", "Creature", vec![]);
    e.state.stack.push(stack(&victim, "pending"));
    assert!(
        e.sacrifice_permanent_for_effect(&victim.instance_id, &source)
            .unwrap()
            .is_empty()
    );
    assert!(
        e.state.stack[0]
            .decisions
            .get("sourceLkiDeparted")
            .is_none()
    );
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[{"kind":"sacrificePermanent","permanent":{"kind":"self"}}],"effects":[]});
    let action = LegalAction {
        id: "synthetic-cost".to_string(),
        kind: ActionKind::ActivateAbility,
        player_id: "player-1".to_string(),
        label: "Pay own sacrifice cost".to_string(),
        card_instance_id: Some(victim.instance_id.clone()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::from([("activatedLifePayment".to_string(), json!(0))]),
        targets: BTreeMap::new(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    };
    let payment = e.prepare_activated_cost_payment(&action, &rule).unwrap();
    let committed = e.commit_activated_cost_payment(&payment).unwrap();
    e.publish_activated_cost_payment(committed);
    assert!(e.permanent_position(&victim.instance_id).is_none());
    assert_eq!(e.state.stack[0].decisions["sourceLkiDeparted"], json!(true));
    assert_eq!(count(&e, "permanentSacrificed"), 1);
}

#[test]
fn linked_returns_wait_for_the_whole_departure_batch_and_reenter_together() {
    let (mut e, source) = position(2);
    let a = add(&mut e, 1, 1, "duration-a", "Artifact", vec![]);
    let b = add(&mut e, 1, 1, "duration-b", "Artifact", vec![]);
    let a = e.mark_linked_battlefield_entry(&a);
    let b = e.mark_linked_battlefield_entry(&b);
    let watch = observer(
        json!({"kind":"permanentEntered","anyController":true,"excludeSource":true,"where":{"kind":"cardTypeContains","value":"Creature"}}),
    );
    assert!(rule_is_executable(&watch));
    let ca = add(&mut e, 0, 0, "captive-a", "Creature", vec![watch.clone()]);
    let cb = add(&mut e, 0, 0, "captive-b", "Creature", vec![watch]);
    e.exile_permanent(&ca.instance_id).unwrap();
    e.exile_permanent(&cb.instance_id).unwrap();
    e.register_linked_exiled_permanent(&a, &ca.instance_id, true, &ca.definition)
        .unwrap();
    e.register_linked_exiled_permanent(&b, &cb.instance_id, true, &cb.definition)
        .unwrap();
    e.sacrifice_effect_collection(
        vec![a.instance_id, b.instance_id],
        &["player-1".to_string()],
        &source,
    )
    .unwrap();
    assert!(e.has_pending_linked_exile_returns());
    assert_eq!(e.state.players[0].exile.len(), 2);
    assert!(e.state.players[0].battlefield.is_empty());
    e.drain_linked_exile_returns(&mut First::default()).unwrap();
    assert_eq!(e.state.players[0].battlefield.len(), 2);
    assert_eq!(
        e.state.stack.len(),
        2,
        "each returned observer sees the other ETB"
    );
    assert!(!e.has_pending_linked_exile_returns());
}

#[test]
fn earthbend_hook_runs_only_after_every_sacrifice_event_is_published() {
    let (mut e, source) = position(2);
    let mut watcher = observer(
        json!({"kind":"permanentEntered","anyController":true,"where":{"kind":"cardTypeContains","value":"Land"}}),
    );
    watcher["condition"] = json!({"kind":"compare","operator":"==","left":{"kind":"countEvents","event":"permanentSacrificed"},"right":{"kind":"integer","value":2}});
    assert!(rule_is_executable(&watcher));
    add(&mut e, 0, 0, "survivor", "Enchantment", vec![watcher]);
    let mut land = add(&mut e, 1, 1, "earthbent", "Land Creature", vec![]);
    land.flags.insert("earthbent".to_string(), true);
    e.state.players[1].battlefield[0] = land.clone();
    let victim = add(&mut e, 1, 1, "victim", "Creature", vec![]);
    e.sacrifice_effect_collection(
        vec![land.instance_id.clone(), victim.instance_id],
        &["player-1".to_string()],
        &source,
    )
    .unwrap();
    let returned = e
        .state
        .events
        .iter()
        .position(|ev| ev.kind == "earthbentLandReturned")
        .unwrap();
    let sacrifices = e
        .state
        .events
        .iter()
        .enumerate()
        .filter(|(_, ev)| ev.kind == "permanentSacrificed")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(sacrifices.len(), 2);
    assert!(sacrifices.iter().all(|i| *i < returned));
    assert_eq!(
        e.state.stack.len(),
        1,
        "ETB condition sees the complete sacrifice event"
    );
    assert!(e.permanent_position(&land.instance_id).is_some());
    // This preserves the old immediate hook, not a claim of the CR701.66 delayed trigger.
}

#[test]
fn collection_and_selection_use_effective_battlefield_types_but_not_printed_card_types() {
    let (mut e, source) = position(2);
    let grant = static_rule(
        json!({"kind":"addCardType","cardType":"Creature","objects":{
        "kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},
        "where":{"kind":"cardTypeContains","value":"Artifact"}}}),
    );
    let remove = static_rule(
        json!({"kind":"removeCardTypeWhile","object":{"kind":"self"},"cardType":"Creature","condition":{
        "kind":"compare","operator":"==","left":{"kind":"integer","value":1},"right":{"kind":"integer","value":1}}}),
    );
    assert!(rule_is_executable(&grant));
    assert!(rule_is_executable(&remove));
    add(&mut e, 1, 1, "animation-source", "Enchantment", vec![grant]);
    let animated = add(&mut e, 1, 1, "animated", "Artifact", vec![]);
    let suppressed = add(&mut e, 1, 1, "suppressed", "Creature", vec![remove]);
    assert!(e.permanent_is_creature(&animated));
    assert!(!e.permanent_is_creature(&suppressed));
    let effects = json!([
        {"kind":"bind","id":"currentCreatures","value":{"kind":"eachPermanent","where":{"kind":"cardTypeContains","value":"Creature"}}},
        {"kind":"chooseCards","id":"selected","player":{"kind":"abilityController"},
         "from":{"kind":"boundObjects","binding":"currentCreatures"},"where":{"kind":"cardTypeContains","value":"Creature"},"count":{"kind":"integer","value":1}}
    ]);
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    e.execute_effects(
        &effects,
        &source,
        &mut bindings,
        &mut decisions,
        &mut First::default(),
    )
    .unwrap();
    assert!(
        matches!(&bindings["currentCreatures"],RuntimeBinding::Objects(objects) if objects==&vec![animated.instance_id.clone()])
    );
    assert_eq!(decisions["selected"], json!([animated.instance_id]));
    // A definition-only view remains printed in other zones.
    assert!(!e.runtime_card_filter_matches(
        &e.state.players[1].battlefield[1].definition,
        &json!({"kind":"cardTypeContains","value":"Creature"}),
        &source,
        &BTreeMap::new(),
        &BTreeMap::new()
    ));
}

#[test]
fn trigger_conditions_observe_the_complete_simultaneous_death_count() {
    let (mut e, source) = position(2);
    let mut watch = observer(
        json!({"kind":"permanentDied","where":{"kind":"cardTypeContains","value":"Creature"},"excludeSource":true}),
    );
    watch["condition"] = json!({"kind":"compare","operator":">=","left":{"kind":"countEvents","event":"permanentDied","player":{"kind":"eachPlayer"}},"right":{"kind":"integer","value":2}});
    assert!(rule_is_executable(&watch));
    let a = add(&mut e, 1, 1, "observer-a", "Creature", vec![watch.clone()]);
    let b = add(&mut e, 1, 1, "observer-b", "Creature", vec![watch]);
    e.sacrifice_effect_collection(
        vec![a.instance_id, b.instance_id],
        &["player-1".to_string()],
        &source,
    )
    .unwrap();
    assert_eq!(count(&e, "permanentDied"), 2);
    assert_eq!(
        e.state.stack.len(),
        2,
        "both observers see two deaths before their intervening condition is checked"
    );
}

#[test]
fn one_or_more_graveyard_observer_receives_one_complete_owner_list() {
    let (mut e, source) = position(2);
    let watch = observer(
        json!({"kind":"cardsEnteredGraveyard","player":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"}}),
    );
    assert!(rule_is_executable(&watch));
    add(&mut e, 1, 1, "observer", "Enchantment", vec![watch]);
    let a = add(&mut e, 1, 1, "victim-a", "Creature", vec![]);
    let b = add(&mut e, 1, 1, "victim-b", "Creature", vec![]);
    e.sacrifice_effect_collection(
        vec![a.instance_id.clone(), b.instance_id.clone()],
        &["player-1".to_string()],
        &source,
    )
    .unwrap();
    assert_eq!(count(&e, "cardsEnteredGraveyard"), 1);
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(
        e.state.stack[0].decisions["triggeringCardIds"],
        json!([a.instance_id, b.instance_id])
    );
}

#[test]
fn mixed_owner_graveyard_lists_are_recorded_before_any_global_condition() {
    let (mut e, source) = position(2);
    let mut watch = observer(
        json!({"kind":"cardsEnteredGraveyard","player":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"}}),
    );
    watch["condition"] = json!({"kind":"compare","operator":">=","left":{"kind":"countEvents","event":"cardsEnteredGraveyard"},"right":{"kind":"integer","value":2}});
    assert!(rule_is_executable(&watch));
    add(
        &mut e,
        0,
        0,
        "owner-zero-observer",
        "Enchantment",
        vec![watch.clone()],
    );
    add(
        &mut e,
        1,
        1,
        "owner-one-observer",
        "Enchantment",
        vec![watch],
    );
    let a = add(&mut e, 0, 1, "victim-a", "Creature", vec![]);
    let b = add(&mut e, 1, 0, "victim-b", "Creature", vec![]);
    e.sacrifice_effect_collection(
        vec![a.instance_id.clone(), b.instance_id.clone()],
        &["player-0".to_string(), "player-1".to_string()],
        &source,
    )
    .unwrap();
    assert_eq!(count(&e, "cardsEnteredGraveyard"), 2);
    assert_eq!(e.state.stack.len(), 2);
    for ability in &e.state.stack {
        assert_eq!(
            ability.decisions["triggeringCardIds"],
            if ability.controller == "player-0" {
                json!([a.instance_id.clone()])
            } else {
                json!([b.instance_id.clone()])
            }
        );
    }
}
