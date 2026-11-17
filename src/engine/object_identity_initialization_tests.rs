use super::object_identity::{ObjectIdentity, object_identity_of};
use super::*;

fn definition(id: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: "Artifact".into(),
        mana_cost: "{0}".into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}
fn engine() -> GameEngine {
    let mut commander = definition("commander");
    commander.is_commander = true;
    let mut outside = definition("outside");
    outside.is_sideboard = true;
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: if i == 0 {
                        vec![
                            definition("a"),
                            definition("b"),
                            definition("c"),
                            commander.clone(),
                            outside.clone(),
                        ]
                    } else {
                        vec![definition("a"), definition("b"), definition("c")]
                    },
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        17001,
    )
    .unwrap()
}
fn identities(state: &GameState) -> BTreeMap<String, ObjectIdentity> {
    state
        .players
        .iter()
        .flat_map(|player| {
            [
                &player.library,
                &player.hand,
                &player.battlefield,
                &player.graveyard,
                &player.exile,
                &player.command_zone,
            ]
        })
        .flat_map(|zone| zone.iter())
        .map(|card| (card.instance_id.clone(), object_identity_of(card).unwrap()))
        .collect()
}
fn card_identity(engine: &GameEngine, id: &str) -> ObjectIdentity {
    let player = &engine.state.players[0];
    [
        &player.library,
        &player.hand,
        &player.battlefield,
        &player.graveyard,
        &player.exile,
        &player.command_zone,
    ]
    .into_iter()
    .flat_map(|zone| zone.iter())
    .find(|card| card.definition.id == id)
    .map(|card| object_identity_of(card).unwrap())
    .unwrap()
}
fn position() -> DeterministicGamePosition {
    DeterministicGamePosition {
        turn_number: 1,
        active_player_id: "p0".into(),
        step: GameStep::PrecombatMain,
        players: vec![
            DeterministicPlayerPosition {
                player_id: "p0".into(),
                life: 20,
                battlefield_definition_ids: vec!["a".into()],
                hand_definition_ids: vec!["b".into()],
                library_size: 2,
                mana_pool: Vec::new(),
            },
            DeterministicPlayerPosition {
                player_id: "p1".into(),
                life: 20,
                battlefield_definition_ids: Vec::new(),
                hand_definition_ids: Vec::new(),
                library_size: 3,
                mana_pool: Vec::new(),
            },
        ],
    }
}

#[test]
fn constructor_admits_all_live_zones_without_admitting_sideboard_or_using_copy_ids() {
    let engine = engine();
    let identities = identities(engine.state());
    assert_eq!(identities.len(), 7);
    assert_eq!(
        identities
            .values()
            .map(|identity| &identity.zone_identity)
            .collect::<BTreeSet<_>>()
            .len(),
        7
    );
    assert_eq!(engine.state.battlefield_incarnation_sequence, 7);
    assert_eq!(engine.copy_sequence, 0);
    assert_eq!(engine.stack_sequence, 0);
    assert_eq!(engine.state.players[0].command_zone.len(), 1);
    assert!(GameEngine::tracked_zone_identity(&engine.state.players[0].sideboard[0]).is_none());
    assert!(engine.state.events.is_empty());
}

#[test]
fn setup_hand_library_roundtrip_changes_identity_but_shuffling_preserves_other_cards() {
    let mut engine = engine();
    let old = card_identity(&engine, "a");
    let unchanged = card_identity(&engine, "b");
    engine
        .configure_fixed_opening_hand("p0", &["a".into()])
        .unwrap();
    let in_hand = object_identity_of(&engine.state.players[0].hand[0]).unwrap();
    assert_eq!(in_hand.instance_id, old.instance_id);
    assert_ne!(in_hand, old);
    assert_eq!(card_identity(&engine, "b"), unchanged);
    engine.configure_training_anchor("p0", 4).unwrap();
    let after = identities(engine.state())[&old.instance_id].clone();
    assert_ne!(after, in_hand);
    assert_ne!(after, old);
    assert_eq!(
        identities(engine.state())[&unchanged.instance_id],
        unchanged
    );
}

#[test]
fn setup_graveyard_and_command_writers_mint_before_return_without_gameplay_events() {
    let mut engine = engine();
    let before = identities(engine.state());
    engine
        .configure_deterministic_graveyard("p0", &["a".into()])
        .unwrap();
    engine
        .configure_deterministic_command_zone("p0", &["b".into()])
        .unwrap();
    for card in engine.state.players[0]
        .graveyard
        .iter()
        .chain(engine.state.players[0].command_zone.iter())
        .filter(|card| card.definition.id != "commander")
    {
        assert_ne!(object_identity_of(card).unwrap(), before[&card.instance_id]);
    }
    assert!(engine.state.events.is_empty());
    assert_eq!(engine.state.battlefield_incarnation_sequence, 9);
}

#[test]
fn rejected_setup_restores_cards_markers_allocator_rng_and_events() {
    let mut engine = engine();
    let before = engine.state.clone();
    let rng = engine.rng_fingerprint_for_search();
    assert!(
        engine
            .configure_fixed_opening_hand("p0", &["a".into(), "absent".into()])
            .is_err()
    );
    assert_eq!(engine.state, before);
    assert_eq!(engine.rng_fingerprint_for_search(), rng);
}

#[test]
fn declaration_uses_admitted_identity_and_roundtrip_never_recaptures() {
    let mut engine = engine();
    engine
        .configure_deterministic_position(&position())
        .unwrap();
    let target = &engine.state.players[0].battlefield[0];
    let captured = object_identity_of(target).unwrap();
    let reference = TargetRef::Permanent {
        instance_id: target.instance_id.clone(),
    };
    let mut decisions = BTreeMap::new();
    engine
        .capture_target_incarnation(&mut decisions, "selected", &reference)
        .unwrap();
    let stack = StackObject {
        id: "stack:identity-test".into(),
        controller: "p0".into(),
        card: target.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: Some(json!({"kind":"activatedAbility"})),
        decisions,
        targets: BTreeMap::from([("selected".into(), reference.clone())]),
    };
    assert!(engine.target_incarnation_is_current(&stack, "selected", &reference));
    let wire = serde_json::to_value(&stack).unwrap();
    let preserved: StackObject = serde_json::from_value(wire).unwrap();
    let state_wire = serde_json::to_value(engine.state()).unwrap();
    let state: GameState = serde_json::from_value(state_wire).unwrap();
    assert_eq!(identities(&state)[&captured.instance_id], captured);
    assert!(engine.target_incarnation_is_current(&preserved, "selected", &reference));
    assert_eq!(preserved.decisions, stack.decisions);
    assert_eq!(
        state.battlefield_incarnation_sequence,
        engine.state.battlefield_incarnation_sequence
    );
}

#[test]
fn shared_battlefield_writer_invalidates_prior_capture_and_control_change_does_not() {
    let mut engine = engine();
    engine
        .configure_deterministic_position(&position())
        .unwrap();
    let id = engine.state.players[0].battlefield[0].instance_id.clone();
    let original = object_identity_of(&engine.state.players[0].battlefield[0]).unwrap();
    engine.change_permanent_controller(&id, "p1").unwrap();
    let (p, i) = engine.permanent_position(&id).unwrap();
    assert_eq!(
        object_identity_of(&engine.state.players[p].battlefield[i]).unwrap(),
        original
    );
    let old_snapshot = engine.state.players[p].battlefield[i].clone();
    engine.exile_permanent(&id).unwrap();
    let owner = engine
        .state
        .players
        .iter()
        .position(|player| player.exile.iter().any(|card| card.instance_id == id))
        .unwrap();
    let index = engine.state.players[owner]
        .exile
        .iter()
        .position(|card| card.instance_id == id)
        .unwrap();
    let returning = engine.state.players[owner].exile.remove(index);
    let clock = engine.state.battlefield_incarnation_sequence;
    engine.commit_prepared_battlefield_entries(vec![(owner, returning)]);
    let (p, i) = engine.permanent_position(&id).unwrap();
    let fresh = object_identity_of(&engine.state.players[p].battlefield[i]).unwrap();
    assert_eq!(fresh.instance_id, original.instance_id);
    assert_ne!(fresh, original);
    assert_eq!(engine.state.battlefield_incarnation_sequence, clock + 1);
    // The local copy is an old source snapshot, not another live carrier.
    assert_eq!(object_identity_of(&old_snapshot).unwrap(), original);
}

#[test]
fn explicit_setup_refuses_a_missing_existing_identity_instead_of_repairing_it() {
    let mut engine = engine();
    GameEngine::clear_tracked_zone_identity(&mut engine.state.players[0].library[0]);
    let before = engine.state.clone();
    assert!(engine.configure_training_anchor("p0", 4).is_err());
    assert_eq!(engine.state, before);
}
#[test]
fn failed_multi_target_capture_preserves_every_decision_and_never_mints() {
    let mut engine = engine();
    engine
        .configure_deterministic_position(&position())
        .unwrap();
    let valid = TargetRef::Permanent {
        instance_id: engine.state.players[0].battlefield[0].instance_id.clone(),
    };
    let mut declaration = CastingDeclaration {
        decisions: BTreeMap::from([("sentinel".into(), json!("keep"))]),
        targets: BTreeMap::from([
            ("first".into(), valid),
            (
                "second".into(),
                TargetRef::Permanent {
                    instance_id: "absent".into(),
                },
            ),
        ]),
        target_order: vec!["first".into(), "second".into()],
    };
    let decisions = declaration.decisions.clone();
    let before = engine.state.clone();
    assert!(
        engine
            .capture_declared_target_incarnations(&mut declaration)
            .is_err()
    );
    assert_eq!(declaration.decisions, decisions);
    assert_eq!(engine.state, before);
}

#[test]
fn missing_target_capture_returns_error_without_replacing_existing_capture_map() {
    let engine = engine();
    let mut decisions = BTreeMap::from([(
        "__deepdeckTargetBattlefieldIdentitiesV1".into(),
        json!({"old":"keep"}),
    )]);
    let original = decisions.clone();
    let target = TargetRef::Permanent {
        instance_id: "absent".into(),
    };
    assert!(
        engine
            .capture_target_incarnation(&mut decisions, "new", &target)
            .is_err()
    );
    assert_eq!(decisions, original);
}
