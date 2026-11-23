use super::super::*;
use serde_json::json;

fn definition(id: &str, kind: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: kind.into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".into(),
        power: Some("2".into()),
        toughness: Some("2".into()),
        rules: Vec::new(),
    }
}

fn position() -> (GameEngine, StackObject) {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
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
        188,
    )
    .unwrap();
    let mut source = card_instance("player-0", &definition("source", "Creature - Scout"), 0);
    GameEngine::assign_tracked_zone_identity(&mut source, "battlefield:original");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("companion", "Creature - Scout"),
        1,
    ));
    engine.admit_explicit_test_fixture_objects().unwrap();
    let stack = StackObject {
        id: "stack:count-source".into(),
        controller: "player-0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn expression() -> Value {
    json!({"kind":"countPermanents", "excludeSource":true,
        "where":{"kind":"cardTypeContains","value":"Creature"}})
}

fn assert_paths(
    engine: &GameEngine,
    stack: &StackObject,
    expression: &Value,
    expected: Option<i32>,
) {
    assert_eq!(
        engine.numeric_expression_with_source(expression, "player-0", Some(&stack.card)),
        expected
    );
    assert_eq!(
        engine.runtime_numeric_expression(expression, stack, &BTreeMap::new(), &stack.decisions),
        expected
    );
}

#[test]
fn current_source_is_excluded_but_absent_source_is_not() {
    let (mut engine, stack) = position();
    assert_paths(&engine, &stack, &expression(), Some(1));
    engine.state.players[0].battlefield.remove(0);
    assert_paths(&engine, &stack, &expression(), Some(1));
}

#[test]
fn returned_same_id_is_a_different_object() {
    let (mut engine, stack) = position();
    GameEngine::assign_tracked_zone_identity(
        &mut engine.state.players[0].battlefield[0],
        "battlefield:returned",
    );
    assert_paths(&engine, &stack, &expression(), Some(2));
}

#[test]
fn predicate_uses_current_type_in_both_directions() {
    let (mut engine, stack) = position();
    engine.state.players[0].battlefield[0].definition.type_line = "Artifact".into();
    assert_paths(&engine, &stack, &expression(), Some(1));
    engine.state.players[0].battlefield[0].definition.type_line = "Creature - Scout".into();
    let mut stale = stack.clone();
    stale.card.definition.type_line = "Artifact".into();
    assert_paths(&engine, &stale, &expression(), Some(1));
}

#[test]
fn control_change_respects_selected_player_and_all_players() {
    let (mut engine, stack) = position();
    let mut source = engine.state.players[0].battlefield.remove(0);
    source.controller = "player-1".into();
    engine.state.players[1].battlefield.push(source);
    assert_paths(&engine, &stack, &expression(), Some(1));
    let mut all = expression();
    all["allPlayers"] = json!(true);
    assert_paths(&engine, &stack, &all, Some(1));
    engine.state.players[1].has_lost = true;
    assert_paths(&engine, &stack, &all, Some(1));
}

#[test]
fn ownership_membership_is_not_snapshot_membership() {
    let (mut engine, stack) = position();
    engine.state.players[0].battlefield[0].owner = "player-1".into();
    let mut owned = expression();
    owned["ownership"] = json!("owned");
    assert_paths(&engine, &stack, &owned, Some(1));
    owned["ownership"] = json!("notOwned");
    assert_paths(&engine, &stack, &owned, Some(0));
}

#[test]
fn pending_source_death_captures_lki_without_reducing_live_count() {
    let (mut engine, stack) = position();
    let source_id = stack.card.instance_id.clone();
    engine.state.stack.push(stack);
    engine
        .move_permanent_to_graveyard(&source_id, "sacrificed")
        .unwrap();
    let departed = engine.state.stack[0].clone();
    assert!(super::source_lki::source_has_departed(&departed));
    assert_paths(&engine, &departed, &expression(), Some(1));
    let mut returned = engine.state.players[0].graveyard.pop().unwrap();
    GameEngine::assign_tracked_zone_identity(&mut returned, "battlefield:returned");
    engine.state.players[0].battlefield.push(returned);
    assert_paths(&engine, &departed, &expression(), Some(2));
}

#[test]
fn untracked_snapshot_is_unresolved_while_direct_live_borrow_is_proven() {
    let (mut engine, mut stack) = position();
    GameEngine::clear_tracked_zone_identity(&mut stack.card);
    GameEngine::clear_tracked_zone_identity(&mut engine.state.players[0].battlefield[0]);
    assert_paths(&engine, &stack, &expression(), None);
    let current = &engine.state.players[0].battlefield[0];
    assert_eq!(
        engine.numeric_expression_with_source(&expression(), "player-0", Some(current)),
        Some(1)
    );
}

#[test]
fn no_exclusion_preserves_count_and_invalid_subtraction_is_unresolved() {
    let (engine, stack) = position();
    let mut plain = expression();
    plain["excludeSource"] = json!(false);
    assert_paths(&engine, &stack, &plain, Some(2));
    assert_eq!(
        engine.exclude_current_source_from_count(0, Some(&stack.card), false, |_, _| true),
        None
    );
}

#[test]
fn decision_dependent_predicate_excludes_the_current_matching_source() {
    let (mut engine, mut stack) = position();
    stack.decisions.insert("tribe".into(), json!("Scout"));
    let query = json!({"kind":"countPermanents", "excludeSource":true,
        "where":{"kind":"chosenCreatureType","decisionId":"tribe"}});
    let evaluate = |engine: &GameEngine| {
        engine.runtime_numeric_expression(&query, &stack, &BTreeMap::new(), &stack.decisions)
    };
    assert_eq!(evaluate(&engine), Some(1));
    GameEngine::assign_tracked_zone_identity(
        &mut engine.state.players[0].battlefield[0],
        "battlefield:returned",
    );
    assert_eq!(evaluate(&engine), Some(2));
    engine.state.players[0].battlefield.remove(0);
    assert_eq!(evaluate(&engine), Some(1));
}

fn setup_position() -> DeterministicGamePosition {
    DeterministicGamePosition {
        turn_number: 1,
        active_player_id: "player-0".into(),
        step: GameStep::PrecombatMain,
        players: (0..2)
            .map(|i| DeterministicPlayerPosition {
                player_id: format!("player-{i}"),
                life: 20,
                battlefield_definition_ids: if i == 0 {
                    vec!["source".into(), "companion".into()]
                } else {
                    vec![]
                },
                hand_definition_ids: vec![],
                library_size: 0,
                mana_pool: vec![],
            })
            .collect(),
    }
}

#[test]
fn deterministic_setup_stamps_before_capture_and_rebuild_creates_fresh_identity() {
    let (mut engine, mut stack) = position();
    GameEngine::clear_tracked_zone_identity(&mut engine.state.players[0].battlefield[0]);
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine
        .configure_deterministic_position(&setup_position())
        .unwrap();
    stack.card = engine.state.players[0].battlefield[0].clone();
    let captured = GameEngine::tracked_zone_identity(&stack.card)
        .unwrap()
        .to_string();
    assert_paths(&engine, &stack, &expression(), Some(1));
    assert_ne!(
        GameEngine::tracked_zone_identity(&engine.state.players[0].battlefield[1]),
        Some(captured.as_str())
    );
    engine
        .configure_deterministic_position(&setup_position())
        .unwrap();
    assert_ne!(
        GameEngine::tracked_zone_identity(&engine.state.players[0].battlefield[0]),
        Some(captured.as_str())
    );
    assert_paths(&engine, &stack, &expression(), Some(2));
    // A new, authoritative trigger capture copies the rebuilt object's stamp.
    let id = engine.put_triggered_ability_on_stack(
        &engine.state.players[0].battlefield[0].clone(),
        json!({"kind":"triggeredAbility", "effects":[]}),
    );
    let id = id
        .published_id()
        .expect("authoritative trigger was published");
    let rebuilt = engine.state.stack.iter().find(|s| s.id == id).unwrap();
    assert_paths(&engine, rebuilt, &expression(), Some(1));
}

#[test]
fn setup_token_has_an_identity_without_publishing_an_entry_event() {
    let (mut engine, _) = position();
    let mut token = definition("setup-token", "Creature - Scout");
    token.is_token = true;
    let events_before = engine.state.events.len();
    engine
        .configure_deterministic_battlefield_token("player-0", 0, token.clone())
        .unwrap();
    engine
        .configure_deterministic_battlefield_token("player-0", 0, token)
        .unwrap();
    let first = &engine.state.players[0].battlefield[0];
    let second = &engine.state.players[0].battlefield[1];
    assert!(GameEngine::tracked_zone_identity(first).is_some());
    assert_ne!(
        GameEngine::tracked_zone_identity(first),
        GameEngine::tracked_zone_identity(second)
    );
    assert_eq!(engine.state.events.len(), events_before);
}

#[test]
fn punching_bag_setup_stamps_both_constructed_sides() {
    let (mut engine, _) = position();
    let mut learner = engine.state.players[0].battlefield.remove(0);
    GameEngine::clear_tracked_zone_identity(&mut learner);
    engine.state.players[0].library.push(learner);
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine
        .configure_punching_bag_position(
            "player-0",
            &["source".into()],
            &[],
            "player-1",
            &[definition("opponent", "Creature - Scout")],
            0,
            false,
        )
        .unwrap();
    let learner =
        GameEngine::tracked_zone_identity(&engine.state.players[0].battlefield[0]).unwrap();
    let opponent =
        GameEngine::tracked_zone_identity(&engine.state.players[1].battlefield[0]).unwrap();
    assert_ne!(learner, opponent);
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "identity proof unexpectedly requested a choice",
        ))
    }
}

fn counted_life(kind: &str) -> Value {
    json!({"kind":"gainLife", "player":{"kind":"abilityController"},
        "amount":{"kind":"countPermanents", "excludeSource":true,
            "where":{"kind":"cardTypeContains","value":kind}}})
}

fn no_other_permanent_guard(kind: &str) -> Value {
    json!({"kind":"compare", "operator":"==", "left":counted_life(kind)["amount"],
        "right":{"kind":"integer","value":0}})
}

#[test]
fn land_play_trigger_captures_prepared_identity_before_etb_publication() {
    let (mut engine, _) = position();
    engine
        .configure_deterministic_position(&setup_position())
        .unwrap();
    engine.state.players[0].battlefield.clear();
    let mut land = definition("land-source", "Land");
    land.rules = vec![json!({"kind":"triggeredAbility", "source":{"kind":"self"},
        "event":{"kind":"landPlayed", "player":{"kind":"abilityController"}},
        "condition":no_other_permanent_guard("Land"),
        "effects":[counted_life("Land")]})];
    engine.state.players[0].hand = vec![card_instance("player-0", &land, 33)];
    engine.state.players[0].land_plays_remaining = 1;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine
        .play_canonical_land(
            &CanonicalLandPlay {
                source_zone: None,
                player_id: "player-0".into(),
                source_definition_id: "land-source".into(),
            },
            &mut NoChoices,
        )
        .unwrap();
    assert_eq!(engine.state.stack.len(), 1);
    let source = &engine.state.stack[0].card;
    let captured =
        GameEngine::tracked_zone_identity(source).expect("landPlayed captured prepared entry");
    assert_eq!(
        Some(captured),
        GameEngine::tracked_zone_identity(&engine.state.players[0].battlefield[0])
    );
    assert_eq!(
        engine.runtime_numeric_expression(
            &counted_life("Land")["amount"],
            &engine.state.stack[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        Some(0)
    );
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn room_entry_door_trigger_captures_same_prepared_incarnation() {
    let (mut engine, _) = position();
    engine
        .configure_deterministic_position(&setup_position())
        .unwrap();
    engine.state.players[0].battlefield.clear();
    let mut room = definition("room-source", "Enchantment - Room // Enchantment - Room");
    room.mana_cost = "{0} // {0}".into();
    room.rules = vec![json!({"kind":"triggeredAbility", "source":{"kind":"self"},
        "roomDoorIndex":0, "event":{"kind":"doorUnlocked", "object":{"kind":"self"}},
        "condition":no_other_permanent_guard("Enchantment"),
        "effects":[counted_life("Enchantment")]})];
    engine.state.players[0].hand = vec![card_instance("player-0", &room, 34)];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|a| {
            a.kind == ActionKind::CastSpell
                && a.decisions.get("roomDoor").and_then(Value::as_u64) == Some(0)
        })
        .expect("Room's first door is castable");
    engine
        .apply_priority_action(&action, &mut NoChoices)
        .unwrap();
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.stack.len(), 1);
    let captured = GameEngine::tracked_zone_identity(&engine.state.stack[0].card)
        .expect("door trigger carries entry identity");
    assert_eq!(
        Some(captured),
        GameEngine::tracked_zone_identity(&engine.state.players[0].battlefield[0])
    );
    assert_eq!(
        engine.runtime_numeric_expression(
            &counted_life("Enchantment")["amount"],
            &engine.state.stack[0],
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        Some(0)
    );
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn true_entry_replaces_nonbattlefield_identity_but_control_change_preserves_it() {
    let (mut engine, mut stack) = position();
    let mut card = engine.state.players[0].battlefield.remove(0);
    GameEngine::assign_tracked_zone_identity(&mut card, "exile:old-object");
    engine.prepare_battlefield_entry_identity(&mut card);
    let identity = GameEngine::tracked_zone_identity(&card)
        .unwrap()
        .to_string();
    assert!(identity.starts_with("object:incarnation:"));
    assert_ne!(identity, "exile:old-object");
    stack.card = card.clone();
    engine.state.players[0].battlefield.push(card);
    engine
        .change_permanent_controller(&stack.card.instance_id, "player-1")
        .unwrap();
    let current = &engine.state.players[1].battlefield[0];
    assert_eq!(
        GameEngine::tracked_zone_identity(current),
        Some(identity.as_str())
    );
    let mut all = expression();
    all["allPlayers"] = json!(true);
    assert_paths(&engine, &stack, &all, Some(1));
}

#[test]
fn actual_blink_gives_new_entry_identity_and_preserves_pending_old_source() {
    let (mut engine, mut stack) = position();
    stack.targets.insert(
        "blinkTarget".into(),
        TargetRef::Permanent {
            instance_id: stack.card.instance_id.clone(),
        },
    );
    engine.state.stack.push(stack.clone());
    engine
        .execute_effect(
            &json!({"kind":"blinkPermanent", "permanent":{"kind":"chosenTarget", "id":"blinkTarget"}}),
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NoChoices,
        )
        .unwrap();
    let pending = &engine.state.stack[0];
    assert!(super::source_lki::source_has_departed(pending));
    let (player, index) = engine.permanent_position(&stack.card.instance_id).unwrap();
    let returned = &engine.state.players[player].battlefield[index];
    assert_ne!(
        GameEngine::tracked_zone_identity(returned),
        GameEngine::tracked_zone_identity(&pending.card)
    );
    assert_eq!(returned.instance_id, pending.card.instance_id);
    assert_paths(&engine, pending, &expression(), Some(2));
}

// Moved from the Oracle-gated tests.rs partition. Keep the original runtime
// assertion: only the two allied creatures contribute to the source's bonus.
#[test]
fn temporary_counted_bonus_excludes_the_ability_source() {
    let (mut engine, stack) = position();
    engine.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("second-ally", "Creature - Soldier"),
        2,
    ));
    engine.state.players[1].battlefield.push(card_instance(
        "player-1",
        &definition("opposing-creature", "Creature - Soldier"),
        3,
    ));
    let count = json!({
        "kind": "countPermanents",
        "player": { "kind": "abilityController" },
        "where": { "kind": "cardTypeContains", "value": "Creature" },
        "excludeSource": true,
    });
    let effect = json!({
        "kind": "modifyPowerToughness",
        "object": { "kind": "self" },
        "power": count.clone(),
        "toughness": count,
        "duration": { "kind": "untilEndOfCurrentTurn" },
    });
    engine
        .execute_effect(
            &effect,
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NoChoices,
        )
        .expect("the counted bonus resolves");
    let source = &engine.state.players[0].battlefield[0];
    assert_eq!(engine.current_power(source), 4);
    assert_eq!(engine.current_toughness(source), 4);
}
