use super::*;

fn setup() -> (GameEngine, StackObject) {
    let card: CardDefinition = serde_json::from_value(json!({"id":"subject", "name":"Subject", "typeLine":"Creature — Beast", "power":"2", "toughness":"2", "manaCost":"{0}"})).unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("P{n}"),
                    starting_life: 20,
                    cards: vec![card.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        251115,
    )
    .unwrap();
    let source = card_instance("p0", &card, 99);
    let installed = engine.install_prepared_battlefield_entries(vec![(0, source)]);
    let source = installed.into_iter().next().unwrap();
    let stack = StackObject {
        id: "subject-frame".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

#[test]
fn source_writes_follow_live_original_incarnation_not_departure_lki_or_returned_carrier() {
    let (mut engine, source) = setup();
    let reference = json!({"kind":"self"});
    assert!(
        matches!(engine.resolve_permanent_action_subject(&reference, &source).unwrap(), PermanentSubjectResolution::Resolved(ids) if ids == [source.card.instance_id.clone()])
    );
    let (player, index) = engine.permanent_position(&source.card.instance_id).unwrap();
    let original_flags = engine.state.players[player].battlefield[index]
        .flags
        .clone();
    engine.state.players[player].battlefield[index]
        .flags
        .clear();
    assert!(
        engine
            .resolve_permanent_action_subject(&reference, &source)
            .is_err(),
        "a present source without identity is not a departed object"
    );
    engine.state.players[player].battlefield[index].flags = original_flags.clone();
    engine.state.players[player].battlefield[index]
        .flags
        .insert(
            format!(
                "{}another-auth-marker",
                zone_change_triggers::TRACKED_ZONE_PREFIX
            ),
            true,
        );
    assert!(
        engine
            .resolve_permanent_action_subject(&reference, &source)
            .is_err(),
        "a present source with ambiguous identity is not a match"
    );
    engine.state.players[player].battlefield[index].flags = original_flags;
    engine
        .change_permanent_controller(&source.card.instance_id, "p1")
        .unwrap();
    assert!(
        matches!(engine.resolve_permanent_action_subject(&reference, &source).unwrap(), PermanentSubjectResolution::Resolved(ids) if ids.len()==1)
    );
    engine.exile_permanent(&source.card.instance_id).unwrap();
    assert!(matches!(
        engine
            .resolve_permanent_action_subject(&reference, &source)
            .unwrap(),
        PermanentSubjectResolution::Unavailable
    ));
    let i = engine.state.players[0]
        .exile
        .iter()
        .position(|card| card.instance_id == source.card.instance_id)
        .unwrap();
    let card = engine.state.players[0].exile.remove(i);
    engine.install_prepared_battlefield_entries(vec![(1, card)]);
    assert!(matches!(
        engine
            .resolve_permanent_action_subject(&reference, &source)
            .unwrap(),
        PermanentSubjectResolution::Unavailable
    ));
    let mut untracked = source.clone();
    untracked.card.flags.clear();
    assert!(
        engine
            .resolve_permanent_action_subject(&reference, &untracked)
            .is_err()
    );
}

#[test]
fn missing_event_or_target_context_errors_and_does_not_become_a_rules_noop() {
    let (engine, source) = setup();
    assert!(
        engine
            .resolve_permanent_action_subject(
                &json!({"kind":"observedEventObject","event":"permanentEntered"}),
                &source
            )
            .is_err()
    );
    assert!(
        engine
            .resolve_permanent_action_subject(
                &json!({"kind":"chosenTarget","id":"unbound"}),
                &source
            )
            .is_err()
    );
    let (mut engine, mut frame) = setup();
    let target = TargetRef::Permanent {
        instance_id: frame.card.instance_id.clone(),
    };
    frame.targets.insert("subject".into(), target.clone());
    frame.ability_rule = Some(
        json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"attacks","object":{"kind":"self"}},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"subject","minimum":1,"maximum":1,"candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}}}]},"effects":[]}),
    );
    engine
        .capture_target_incarnation(&mut frame.decisions, "subject", &target)
        .unwrap();
    let context = engine.target_numeric_context(&frame);
    engine
        .with_target_numeric_context(context, |engine| {
            assert_eq!(
                engine.target_was_legal_at_resolution_start(&frame, "subject"),
                Some(true)
            );
            let (p, i) = engine.permanent_position(&frame.card.instance_id).unwrap();
            engine.state.players[p].battlefield[i].flags.clear();
            assert!(
                engine
                    .resolve_permanent_action_subject(
                        &json!({"kind":"chosenTarget","id":"subject"}),
                        &frame
                    )
                    .is_err(),
                "live identity lost after legal snapshot is an error"
            );
            Ok(())
        })
        .unwrap();
    let (mut observed_engine, mut observed_frame) = setup();
    entry_observation::EntryObservation::capture(&observed_frame.card)
        .add_object_decision(&mut observed_frame.decisions);
    let observed = json!({"kind":"observedEventObject","event":"permanentEntered"});
    assert!(
        matches!(observed_engine.resolve_permanent_action_subject(&observed, &observed_frame).unwrap(), PermanentSubjectResolution::Resolved(ids) if ids.len()==1)
    );
    let (p, i) = observed_engine
        .permanent_position(&observed_frame.card.instance_id)
        .unwrap();
    observed_engine.state.players[p].battlefield[i]
        .flags
        .clear();
    assert!(
        observed_engine
            .resolve_permanent_action_subject(&observed, &observed_frame)
            .is_err(),
        "missing live observed identity is not a departure"
    );

    assert!(matches!(engine.resolve_permanent_action_subject(&json!({"kind":"eachPermanent","where":{"kind":"cardTypeContains","value":"Creature"}}), &source).unwrap(), PermanentSubjectResolution::Historical));
}

#[test]
fn observed_permanent_actions_validate_only_inside_their_producer_scope() {
    let observed = json!({"kind":"observedEventObject","event":"permanentEntered"});
    let actions = vec![
        json!({"kind":"putCounters","permanent":observed,"counter":"charge","count":{"kind":"integer","value":1}}),
        json!({"kind":"grantKeyword","object":observed,"keyword":"flying","duration":{"kind":"untilEndOfCurrentTurn"}}),
        json!({"kind":"modifyPowerToughness","object":observed,"power":{"kind":"integer","value":2},"toughness":{"kind":"integer","value":1},"duration":{"kind":"untilEndOfCurrentTurn"}}),
    ];
    assert!(actions.iter().all(effect_supported));
    let rule = json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"permanentEntered","where":{"kind":"cardTypeContains","value":"Creature"},"player":{"kind":"controllerOf","object":{"kind":"self"}}},"effects":actions});
    assert!(
        rule_supported(&rule),
        "all three ordinary actions have an event-object producer"
    );
    let missing_producer =
        json!({"kind":"spellAbility","source":{"kind":"self"},"effects":rule["effects"]});
    assert!(
        !rule_supported(&missing_producer),
        "shape admission never invents an observed-object producer"
    );
    let unsupported_action = json!({"kind":"grantAbility","object":observed,"duration":{"kind":"permanent"},"ability":{"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"enterBattlefield","object":{"kind":"self"}},"effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":1}}]}});
    assert!(
        !effect_supported(&unsupported_action),
        "grantAbility has no authenticated observed-subject execution path"
    );
}
