use super::super::*;
use super::look_at_face_down_supported;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{2}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        rules: Vec::new(),
    }
}

fn engine(players: usize) -> GameEngine {
    let filler = definition("filler", "Basic Land - Island");
    GameEngine::new(
        GameSetup {
            players: (0..players)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        708,
    )
    .expect("hidden visibility test setup is valid")
}

fn stack_object(controller: &str) -> StackObject {
    StackObject {
        id: "stack:face-down-look".to_string(),
        controller: controller.to_string(),
        card: card_instance(controller, &definition("look-source", "Creature - Bird"), 0),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// A face-down 2/2 of `controller`; returns its id.
fn put_face_down(engine: &mut GameEngine, controller: &str) -> String {
    let mut card = card_instance(
        controller,
        &definition("hidden-dragon", "Creature - Dragon"),
        7,
    );
    card.flags.insert("faceDown".to_string(), true);
    let id = card.instance_id.clone();
    let index = engine.player_index(controller).expect("controller exists");
    engine.state.players[index].battlefield.push(card);
    id
}

fn look(engine: &mut GameEngine, viewer_controller: &str, looked: &str) {
    let bindings = BTreeMap::from([(
        "looked".to_string(),
        RuntimeBinding::Objects(vec![looked.to_string()]),
    )]);
    engine
        .look_at_face_down_permanents(
            &json!({
                "kind": "lookAtFaceDownPermanents",
                "objects": {"kind": "boundObjects", "binding": "looked"},
                "viewer": {"kind": "abilityController"},
            }),
            &stack_object(viewer_controller),
            &bindings,
            &BTreeMap::new(),
        )
        .expect("the look resolves");
}

/// CR 708.5: the controller always sees its face-down permanent; a look
/// grants its front face to the looking player only, never to the others.
#[test]
fn face_down_look_grants_visibility_to_the_viewer_only() {
    let mut engine = engine(3);
    let hidden = put_face_down(&mut engine, "player-1");
    assert!(engine.face_down_permanent_visible_to("player-1", &hidden));
    assert!(!engine.face_down_permanent_visible_to("player-0", &hidden));
    assert!(!engine.face_down_permanent_visible_to("player-2", &hidden));
    look(&mut engine, "player-0", &hidden);
    assert!(engine.face_down_permanent_visible_to("player-0", &hidden));
    assert!(!engine.face_down_permanent_visible_to("player-2", &hidden));
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "faceDownPermanentLookedAt"
            && event.player_id.as_deref() == Some("player-0")
            && event.card_instance_id.as_deref() == Some(hidden.as_str())
    }));
    // A second look records the same knowledge once.
    look(&mut engine, "player-0", &hidden);
    assert_eq!(
        engine
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| value_kind(modifier) == Some("knownFaceDownPermanent"))
            .count(),
        1
    );
}

/// A permanent that is not face down hides nothing and a look at it records
/// no knowledge; a look ends with its face-down object (CR 400.7, 708.8).
#[test]
fn face_up_permanents_record_nothing_and_stale_looks_are_forgotten() {
    let mut engine = engine(2);
    let hidden = put_face_down(&mut engine, "player-1");
    look(&mut engine, "player-0", &hidden);
    let index = engine.player_index("player-1").expect("player-1 exists");
    engine.state.players[index].battlefield[0]
        .flags
        .remove("faceDown");
    assert!(engine.face_down_permanent_visible_to("player-0", &hidden));
    engine.forget_stale_face_down_looks();
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some("knownFaceDownPermanent"))
    );
    // Turned face down again: the earlier look no longer reveals it.
    engine.state.players[index].battlefield[0]
        .flags
        .insert("faceDown".to_string(), true);
    assert!(!engine.face_down_permanent_visible_to("player-0", &hidden));
    // A look at a face-up permanent learns nothing.
    engine.state.players[index].battlefield[0]
        .flags
        .remove("faceDown");
    look(&mut engine, "player-0", &hidden);
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some("knownFaceDownPermanent"))
    );
}

/// The card-name suggestions a player sees omit a face-down permanent's
/// name until that player looked at it.
#[test]
fn name_suggestions_follow_face_down_visibility() {
    let mut engine = engine(2);
    let hidden = put_face_down(&mut engine, "player-1");
    let creature = json!({"kind": "cardTypeContains", "value": "Creature"});
    let names = engine.visible_card_name_suggestions("player-0", &creature, "");
    assert!(!names.contains(&"hidden-dragon".to_string()));
    look(&mut engine, "player-0", &hidden);
    let names = engine.visible_card_name_suggestions("player-0", &creature, "");
    assert!(names.contains(&"hidden-dragon".to_string()));
}

#[test]
fn face_down_look_validation_requires_a_viewer_and_objects() {
    let effect = json!({
        "kind": "lookAtFaceDownPermanents",
        "objects": {"kind": "chosenTargets", "id": "targetLookedPermanent"},
        "viewer": {"kind": "abilityController"},
    });
    assert!(look_at_face_down_supported(&effect));
    assert!(effect_supported(&effect));
    let mut population = effect.clone();
    population["viewer"] = json!({"kind": "eachPlayer"});
    assert!(!look_at_face_down_supported(&population));
    let mut missing = effect;
    missing["objects"] = json!({"kind": "unknownObjects"});
    assert!(!look_at_face_down_supported(&missing));
}
