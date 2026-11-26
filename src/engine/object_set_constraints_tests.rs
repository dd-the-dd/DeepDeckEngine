//! Runtime proofs for the set constraints of `object_set_constraints` and
//! for the combat relations the shared object query lowers for a subject
//! relative (`target creature it's blocking`).
use super::super::runtime_object_filter::ObjectFilterView;
use super::super::*;
use super::SAME_PERMANENT_CONTROLLER;

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: mana_cost.to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", "");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        1108,
    )
    .expect("object set constraint setup is valid")
}

fn permanent_ref(card: &CardInstance) -> TargetRef {
    TargetRef::Permanent {
        instance_id: card.instance_id.clone(),
    }
}

/// `Choose two target creatures controlled by the same player`, as the
/// parser lowers it: the set constraint moved to the slot's
/// `selectionConstraint` (CR 601.2c).
fn same_controller_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "permanentEntered", "anyController": true, "where": null },
        "declaration": { "kind": "castingDeclaration", "decisions": [{
            "id": "targetCreatures", "kind": "chooseTargets", "minimum": 2, "maximum": 2,
            "candidates": {
                "kind": "permanents",
                "where": { "kind": "cardTypeContains", "value": "Creature" }
            },
            "selectionConstraint": { "kind": SAME_PERMANENT_CONTROLLER }
        }] },
        "effects": []
    })
}

#[test]
fn same_controller_constraint_is_a_supported_selection_constraint() {
    assert!(target_selection_constraint_supported(
        &json!({ "kind": SAME_PERMANENT_CONTROLLER })
    ));
    assert!(target_selection_constraint_supported(&json!({
        "kind": "allOf",
        "constraints": [
            { "kind": SAME_PERMANENT_CONTROLLER },
            { "kind": "distinctCardNames" }
        ]
    })));
}

#[test]
fn same_controller_holds_for_one_controller_and_fails_across_controllers() {
    let mut engine = engine();
    let first = card_instance(
        "player-1",
        &definition("first", "Creature - Bear", "{1}"),
        1,
    );
    let second = card_instance(
        "player-1",
        &definition("second", "Creature - Bear", "{1}"),
        2,
    );
    let mine = card_instance("player-0", &definition("mine", "Creature - Bear", "{1}"), 3);
    let in_graveyard = card_instance("player-1", &definition("gone", "Creature - Bear", "{1}"), 4);
    engine.state.players[1]
        .battlefield
        .extend([first.clone(), second.clone()]);
    engine.state.players[0].battlefield.push(mine.clone());
    engine.state.players[1].graveyard.push(in_graveyard.clone());

    assert!(
        engine.selection_has_same_permanent_controller(&[
            permanent_ref(&first),
            permanent_ref(&second)
        ])
    );
    assert!(
        !engine.selection_has_same_permanent_controller(&[
            permanent_ref(&first),
            permanent_ref(&mine)
        ])
    );
    // A member that left the battlefield is no permanent with a controller.
    assert!(!engine.selection_has_same_permanent_controller(&[
        permanent_ref(&first),
        permanent_ref(&in_graveyard)
    ]));
    // A control change between declaration and resolution breaks the set
    // (CR 608.2b): the controllers are read where the permanents are now.
    engine.state.players[1].battlefield[1].controller = "player-0".to_string();
    assert!(
        !engine.selection_has_same_permanent_controller(&[
            permanent_ref(&first),
            permanent_ref(&second)
        ])
    );
}

#[test]
fn same_controller_restricts_a_new_choice_to_the_other_targets_controller() {
    let mut engine = engine();
    let source = card_instance("player-0", &definition("source", "Sorcery", "{1}{B}"), 1);
    let first = card_instance(
        "player-1",
        &definition("first", "Creature - Bear", "{1}"),
        2,
    );
    let second = card_instance(
        "player-1",
        &definition("second", "Creature - Bear", "{1}"),
        3,
    );
    let third = card_instance(
        "player-1",
        &definition("third", "Creature - Bear", "{1}"),
        4,
    );
    let mine = card_instance("player-0", &definition("mine", "Creature - Bear", "{1}"), 5);
    engine.state.players[1]
        .battlefield
        .extend([first.clone(), second.clone(), third.clone()]);
    engine.state.players[0].battlefield.push(mine.clone());
    let mut stack = StackObject {
        id: "stack:same-controller".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(same_controller_rule()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    stack
        .targets
        .insert("targetCreatures:0".to_string(), permanent_ref(&first));
    stack
        .targets
        .insert("targetCreatures:1".to_string(), permanent_ref(&second));
    stack.decisions.insert(
        "targetCreatures".to_string(),
        json!([permanent_ref(&first), permanent_ref(&second)]),
    );

    let replacements = engine.retarget_candidates_for_stack_target(&stack, "targetCreatures:1");
    assert!(
        replacements.contains(&permanent_ref(&third)),
        "a creature of the same controller keeps the set legal"
    );
    assert!(
        !replacements.contains(&permanent_ref(&mine)),
        "a creature of another controller breaks the joint constraint"
    );
}

/// `target creature it's blocking` lowers to `blockedBy` anchored on the
/// source. Tinder Wall is sacrificed as a cost, so on resolution the anchor
/// is gone: the block pair is read from the declaration of this turn (last
/// known information, CR 608.2h), and an unblocked attacker never matches.
#[test]
fn blocked_by_a_departed_anchor_reads_the_declared_block() {
    let mut engine = engine();
    let wall = card_instance(
        "player-0",
        &definition("wall", "Creature - Plant Wall", "{G}"),
        1,
    );
    let mut blocked = card_instance(
        "player-1",
        &definition("blocked", "Creature - Bear", "{1}"),
        2,
    );
    let mut unblocked = card_instance("player-1", &definition("free", "Creature - Bear", "{1}"), 3);
    blocked.flags.insert("attacking".to_string(), true);
    unblocked.flags.insert("attacking".to_string(), true);
    engine.state.players[0].battlefield.push(wall.clone());
    engine.state.players[1]
        .battlefield
        .extend([blocked.clone(), unblocked.clone()]);
    engine.state.combat.blockers.push(DeclaredBlocker {
        attacker_id: blocked.instance_id.clone(),
        blocker_id: wall.instance_id.clone(),
    });
    let blocked_identity = engine.object_identity_value(&blocked.instance_id);
    let wall_identity = engine.object_identity_value(&wall.instance_id);
    engine.record_event(
        "blockerDeclared",
        Some("player-0".to_string()),
        Some(wall.instance_id.clone()),
        json!({
            "attackerId": blocked.instance_id.clone(),
            "attackerIdentity": blocked_identity,
            "objectIdentity": wall_identity,
        }),
    );
    let relation = json!({
        "kind": "blockedBy",
        "object": { "kind": "self" },
        "anchorIds": [wall.instance_id.clone()],
    });
    let view = |engine: &GameEngine, id: &str| {
        engine
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .find(|card| card.instance_id == id)
            .cloned()
            .expect("attacker on the battlefield")
    };

    let current = view(&engine, &blocked.instance_id);
    assert!(engine.eval_object_relation(&relation, ObjectFilterView::Permanent(&current)));
    let free = view(&engine, &unblocked.instance_id);
    assert!(!engine.eval_object_relation(&relation, ObjectFilterView::Permanent(&free)));

    // Sacrificed as the cost: the wall leaves, its block pair leaves combat.
    engine.state.players[0]
        .battlefield
        .retain(|card| card.instance_id != wall.instance_id);
    engine
        .state
        .combat
        .blockers
        .retain(|block| block.blocker_id != wall.instance_id);
    let current = view(&engine, &blocked.instance_id);
    assert!(
        engine.eval_object_relation(&relation, ObjectFilterView::Permanent(&current)),
        "the departed blocker's declared block still names the attacker"
    );
    let free = view(&engine, &unblocked.instance_id);
    assert!(!engine.eval_object_relation(&relation, ObjectFilterView::Permanent(&free)));
}
