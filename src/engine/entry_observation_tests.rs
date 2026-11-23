use super::*;

fn definition(id: &str, kind: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: kind.into(),
        mana_cost: String::new(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        rules,
    }
}
fn rule() -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentEntered","where":null,
            "player":{"kind":"controllerOf","object":{"kind":"self"}},"entryState":{"tapped":true}},
        "effects":[{"kind":"untapPermanent","permanent":{"kind":"observedEventObject","event":"permanentEntered"}}]})
}
fn make_engine(watchers: usize) -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("Player{n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Land", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        183,
    )
    .unwrap();
    for n in 0..watchers {
        let mut watcher = card_instance(
            "p0",
            &definition(&format!("observer{n}"), "Artifact", vec![rule()]),
            n,
        );
        watcher.tapped = true;
        engine.prepare_battlefield_entry_identity(&mut watcher);
        engine.state.players[0].battlefield.push(watcher);
    }
    engine
}
fn enter(engine: &mut GameEngine, player: usize, tapped: bool, stun: i32) -> String {
    let mut card = card_instance(
        &format!("p{player}"),
        &definition("entrant", "Land", Vec::new()),
        80,
    );
    card.tapped = tapped;
    if stun > 0 {
        card.counters.insert("stun".into(), stun);
    }
    let id = card.instance_id.clone();
    let installed = engine.install_prepared_battlefield_entries(vec![(player, card)]);
    for source in installed {
        engine.publish_prepared_enter_battlefield_triggers(&source);
    }
    id
}
#[derive(Default)]
struct NoChoices {
    calls: usize,
}
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Err(EngineError::new(
            "unexpected event-observation target/choice",
        ))
    }
}

#[test]
fn tapped_entry_captures_non_target_object_and_does_not_untap_the_observer() {
    let mut engine = make_engine(1);
    let id = enter(&mut engine, 0, true, 0);
    assert_eq!(engine.state.stack.len(), 1);
    assert!(engine.state.stack[0].targets.is_empty());
    assert!(
        engine.state.stack[0]
            .ability_rule
            .as_ref()
            .unwrap()
            .get("declaration")
            .is_none()
    );
    let captured = engine.state.stack[0].decisions[OBSERVED_OBJECT_DECISION].clone();
    assert_eq!(captured["identity"]["instanceId"], id);
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    let (p, i) = engine.permanent_position(&id).unwrap();
    assert!(!engine.state.players[p].battlefield[i].tapped);
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|c| c.definition.id == "observer0")
            .unwrap()
            .tapped
    );
    assert_eq!(provider.calls, 0);
}

#[test]
fn ordinary_decision_ids_do_not_overwrite_the_internal_event_capture() {
    let mut engine = make_engine(1);
    let id = enter(&mut engine, 0, true, 0);
    engine.state.stack[0]
        .decisions
        .insert("observedEventObject".into(), json!("ordinary chosen value"));
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    let (player, index) = engine.permanent_position(&id).unwrap();
    assert!(!engine.state.players[player].battlefield[index].tapped);
    assert_eq!(provider.calls, 0);
}

#[test]
fn untapped_and_opponent_entries_do_not_match_the_observed_entry_event() {
    for (player, tapped) in [(0, false), (1, true)] {
        let mut engine = make_engine(1);
        enter(&mut engine, player, tapped, 0);
        assert!(engine.state.stack.is_empty());
    }
}

#[test]
fn source_and_filtered_entry_shapes_share_one_frozen_state_observation() {
    let mut card = card_instance("p0", &definition("observer", "Artifact", Vec::new()), 81);
    let source_event =
        json!({"kind":"enterBattlefield","object":{"kind":"self"},"entryState":{"tapped":true}});
    let filtered_event = json!({"kind":"permanentEntered","where":null,"anyController":true,"entryState":{"tapped":true}});
    let untapped = EntryObservation::capture(&card);
    assert!(!untapped.matches_state(&source_event));
    assert!(!untapped.matches_state(&filtered_event));
    card.tapped = true;
    let tapped = EntryObservation::capture(&card);
    card.tapped = false;
    assert!(!card.tapped);
    assert!(tapped.matches_state(&source_event));
    assert!(tapped.matches_state(&filtered_event));
    assert!(!entry_state_supported(
        &json!({"kind":"sourceCycled","object":{"kind":"self"},"entryState":{"tapped":true}})
    ));
}

#[test]
fn each_observer_uses_original_tapped_state_and_the_shared_stun_replacement() {
    let mut engine = make_engine(2);
    let id = enter(&mut engine, 0, true, 1);
    assert_eq!(engine.state.stack.len(), 2);
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    let (p, i) = engine.permanent_position(&id).unwrap();
    assert!(engine.state.players[p].battlefield[i].tapped);
    assert_eq!(
        engine.state.players[p].battlefield[i]
            .counters
            .get("stun")
            .copied()
            .unwrap_or(0),
        0
    );
    engine.resolve_top_stack(&mut provider).unwrap();
    assert!(!engine.state.players[p].battlefield[i].tapped);
    assert_eq!(provider.calls, 0);
}

#[test]
fn control_change_preserves_the_observed_object_but_reentry_does_not() {
    let mut engine = make_engine(1);
    let id = enter(&mut engine, 0, true, 0);
    let (p, i) = engine.permanent_position(&id).unwrap();
    let mut card = engine.state.players[p].battlefield.remove(i);
    card.controller = "p1".into();
    engine.state.players[1].battlefield.push(card);
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    let (p, i) = engine.permanent_position(&id).unwrap();
    assert_eq!(p, 1);
    assert!(!engine.state.players[p].battlefield[i].tapped);

    let mut engine = make_engine(1);
    let id = enter(&mut engine, 0, true, 0);
    let original = engine.state.stack[0].decisions[OBSERVED_OBJECT_DECISION].clone();
    let movement = engine.install_permanent_exile_move(&id).unwrap().unwrap();
    engine
        .publish_installed_battlefield_departure(&movement, "effect")
        .unwrap();
    let index = engine.state.players[0]
        .exile
        .iter()
        .position(|c| c.instance_id == id)
        .unwrap();
    let mut card = engine.state.players[0].exile.remove(index);
    card.tapped = false;
    let entering = engine.install_prepared_battlefield_entries(vec![(0, card)]);
    for source in entering {
        engine.publish_prepared_enter_battlefield_triggers(&source);
    }
    let (p, i) = engine.permanent_position(&id).unwrap();
    assert_ne!(
        GameEngine::tracked_zone_identity(&engine.state.players[p].battlefield[i]),
        original["identity"]["zoneIdentity"].as_str()
    );
    // Establish a tapped new incarnation after its untapped entry; the old
    // trigger must not act on it, regardless of the shared physical card ID.
    engine.state.players[p].battlefield[i].tapped = true;
    engine.resolve_top_stack(&mut provider).unwrap();
    assert!(engine.state.players[p].battlefield[i].tapped);
    assert_eq!(provider.calls, 0);
}

#[test]
fn missing_or_malformed_observation_is_an_error_and_prior_effects_roll_back() {
    for decisions in [
        BTreeMap::new(),
        BTreeMap::from([(
            OBSERVED_OBJECT_DECISION.into(),
            json!({"event":"permanentDied"}),
        )]),
    ] {
        let mut engine = make_engine(1);
        let mut program = rule();
        program["effects"].as_array_mut().unwrap().insert(0,json!({"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}));
        let before = engine.state.clone();
        let mut provider = NoChoices::default();
        let error = engine
            .execute_canonical_rule(
                CanonicalRuleExecution {
                    controller_id: "p0".into(),
                    source_definition_id: "observer0".into(),
                    rule: program,
                    decisions,
                    targets: BTreeMap::new(),
                },
                &mut provider,
            )
            .unwrap_err();
        assert!(error.to_string().contains("event-object capture"));
        assert_eq!(engine.state, before);
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn strict_reference_parameters_and_producer_scope_reject_orphans_and_nested_leaks() {
    assert!(rule_is_executable(&rule()));
    let mut spell = rule();
    spell["kind"] = json!("spellAbility");
    assert!(!rule_is_executable(&spell));
    let mut unknown = rule();
    unknown["effects"][0]["permanent"]["event"] = json!("permanentDied");
    assert!(!rule_is_executable(&unknown));
    let mut malformed = rule();
    malformed["event"]["entryState"]["counter"] = json!("stun");
    assert!(!rule_is_executable(&malformed));
    let mut extra = rule();
    extra["effects"][0]["permanent"]["slot"] = json!(0);
    assert!(!rule_is_executable(&extra));
    let nested = json!({"kind":"triggeredAbility","event":rule()["event"],"effects":[{"kind":"opaqueContainer","ability":spell}]});
    assert!(!reference_scope_supported(&nested));
}

#[test]
fn entry_occurrence_has_one_observed_object_even_when_multiple_or_branches_match() {
    let mut engine = make_engine(0);
    let mut program = rule();
    program["event"] = json!({"kind":"oneOf","events":[
        {"kind":"enterBattlefield","object":{"kind":"self"},"entryState":{"tapped":true}},
        {"kind":"permanentEntered","anyController":true,"where":{"kind":"cardTypeContains","value":"Artifact"},"entryState":{"tapped":true}}
    ]});
    assert!(rule_is_executable(&program));
    let mut card = card_instance(
        "p0",
        &definition("or-entry-source", "Artifact", vec![program]),
        95,
    );
    card.tapped = true;
    let id = card.instance_id.clone();
    let installed = engine.install_prepared_battlefield_entries(vec![(0, card)]);
    for source in installed {
        engine.publish_prepared_enter_battlefield_triggers(&source);
    }
    // Self AND filtered branches match this one occurrence. OR means one
    // trigger, not two objects, two targets or two callbacks.
    assert_eq!(engine.state.stack.len(), 1);
    assert!(engine.state.stack[0].targets.is_empty());
    assert_eq!(
        engine.state.stack[0].decisions[OBSERVED_OBJECT_DECISION]["identity"]["instanceId"],
        id
    );
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    let (player, index) = engine.permanent_position(&id).unwrap();
    assert!(!engine.state.players[player].battlefield[index].tapped);
    assert_eq!(provider.calls, 0);
}

#[test]
fn all_entry_branches_produce_a_reference_but_entry_or_death_does_not() {
    let mut program = rule();
    program["event"] = json!({"kind":"enterBattlefield","object":{"kind":"self"}});
    assert!(rule_is_executable(&program));
    let source_event = program["event"].clone();
    program["event"] = json!({"kind":"oneOf","events":[
        source_event,
        {"kind":"permanentEntered","anyController":true,"where":null}
    ]});
    assert!(rule_is_executable(&program));
    program["event"]["events"][1] = json!({"kind":"permanentDied","object":{"kind":"self"}});
    assert!(!rule_is_executable(&program));
    assert!(!reference_scope_supported(&program));
}

/// Grave Peril's shape: `When a nonblack creature enters, sacrifice this
/// enchantment. If you do, destroy that creature.` The destroy reads the
/// creature the entry event observed, never a target.
#[test]
fn performed_sacrifice_destroys_the_observed_entering_creature() {
    let peril = json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentEntered","anyController":true,
            "where":{"kind":"cardTypeContains","value":"Creature"}},
        "effects":[{"kind":"performAction",
            "action":{"kind":"sacrificePermanent","permanent":{"kind":"self"}},
            "onPerformed":[{"kind":"destroyPermanent",
                "permanent":{"kind":"observedEventObject","event":"permanentEntered"}}]}]});
    assert!(rule_is_executable(&peril));
    let mut engine = make_engine(0);
    let mut source = card_instance("p0", &definition("peril", "Enchantment", vec![peril]), 7);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source);
    let mut creature = card_instance("p1", &definition("bear", "Creature - Bear", Vec::new()), 81);
    creature.definition.power = Some("2".into());
    creature.definition.toughness = Some("2".into());
    let id = creature.instance_id.clone();
    for installed in engine.install_prepared_battlefield_entries(vec![(1, creature)]) {
        engine.publish_prepared_enter_battlefield_triggers(&installed);
    }
    assert_eq!(engine.state.stack.len(), 1);
    let mut provider = NoChoices::default();
    engine.resolve_top_stack(&mut provider).unwrap();
    assert!(
        engine.permanent_position(&id).is_none(),
        "the observed creature is destroyed"
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|card| card.definition.id != "peril"),
        "the enchantment was sacrificed"
    );
    assert_eq!(provider.calls, 0);
}
