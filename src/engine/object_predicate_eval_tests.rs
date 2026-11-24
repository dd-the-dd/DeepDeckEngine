//! Object-predicate atoms that read game state beyond a definition: block
//! status (CR 509.1h), read through the same permanent-filter evaluators
//! every collection, target and event consumer uses, and the relation of a
//! spell target candidate to its own targets (CR 115.1).

use super::super::{
    CardDefinition, DeclaredAttacker, DeclaredBlocker, GameSetup, PlayerDeck, StackObject,
    TargetRef, card_instance,
};
use super::*;
use serde_json::{Value, json};

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": "{0}",
        "power": "2",
        "toughness": "2",
        "rules": [],
    }))
    .expect("synthetic definition deserializes")
}

fn engine() -> GameEngine {
    let land: CardDefinition =
        serde_json::from_value(json!({"id":"land","name":"Land","typeLine":"Land","rules":[]}))
            .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![land.clone(); 4],
                })
                .collect(),
        },
        509,
    )
    .unwrap();
    for (player, id) in [(0, "attacker"), (0, "idle"), (1, "blocker")] {
        let mut creature = card_instance(
            &format!("player-{player}"),
            &definition(id, "Creature - Bear"),
            0,
        );
        creature.instance_id = id.to_string();
        engine.state.players[player].battlefield.push(creature);
    }
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "attacker".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
    engine
}

fn permanent<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("synthetic permanent")
}

/// The status through both permanent evaluators: effective characteristics
/// (collections, targets) and permanent state (event and condition views).
fn status(engine: &GameEngine, id: &str, kind: &str) -> bool {
    let filter: Value = json!({ "kind": kind });
    let card = permanent(engine, id);
    let effective = engine.permanent_matches_effective_filter(card, &filter);
    assert_eq!(
        effective,
        engine.permanent_state_matches(card, &filter),
        "both evaluators read the same block status for {id} {kind}"
    );
    effective
}

fn declare_block(engine: &mut GameEngine) {
    engine.state.combat.blockers.push(DeclaredBlocker {
        attacker_id: "attacker".to_string(),
        blocker_id: "blocker".to_string(),
    });
    let identity = engine.object_identity_value("attacker");
    engine.record_event(
        "blockerDeclared",
        Some("player-1".to_string()),
        Some("blocker".to_string()),
        json!({ "attackerId": "attacker", "attackerIdentity": identity }),
    );
}

#[test]
fn block_status_follows_declaration_and_survives_blocker_removal() {
    let mut engine = engine();
    assert!(status(&engine, "attacker", "isUnblockedAttacker"));
    assert!(!status(&engine, "attacker", "isBlocked"));
    // A creature that is not attacking is neither blocked nor unblocked.
    assert!(!status(&engine, "idle", "isBlocked"));
    assert!(!status(&engine, "idle", "isUnblockedAttacker"));

    declare_block(&mut engine);
    assert!(status(&engine, "attacker", "isBlocked"));
    assert!(!status(&engine, "attacker", "isUnblockedAttacker"));

    // CR 509.1h: removing the blocker from combat leaves the attacker blocked.
    engine.state.combat.blockers.clear();
    assert!(status(&engine, "attacker", "isBlocked"));
    assert!(!status(&engine, "attacker", "isUnblockedAttacker"));

    // A composed filter (`blocked creature`) reads the same atom.
    let blocked_creature = json!({"kind":"and","operands":[
        {"kind":"cardTypeContains","value":"Creature"},
        {"kind":"isBlocked"}
    ]});
    assert!(
        engine
            .permanent_matches_effective_filter(permanent(&engine, "attacker"), &blocked_creature)
    );
    assert!(
        !engine
            .permanent_matches_effective_filter(permanent(&engine, "blocker"), &blocked_creature)
    );
}

#[test]
fn block_status_is_a_supported_filter_and_refused_by_static_selectors() {
    for kind in ["isBlocked", "isUnblockedAttacker"] {
        assert!(super::super::card_filter_supported(
            &json!({ "kind": kind })
        ));
    }
    // A static selector is matched without the event log; the blocked status
    // needs it, so the selector is refused rather than read as false.
    assert!(!super::super::static_object_selector_supported(&json!({
        "kind": "permanents",
        "where": {"kind":"isBlocked"},
    })));
}

fn instant_on_stack(controller: &str, targets: Vec<(&str, TargetRef)>) -> StackObject {
    let mut card = card_instance(controller, &definition("bolt", "Instant"), 0);
    card.instance_id = "bolt".to_string();
    StackObject {
        id: "stack-bolt".to_string(),
        controller: controller.to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: Default::default(),
        targets: targets
            .into_iter()
            .map(|(id, target)| (id.to_string(), target))
            .collect(),
    }
}

#[test]
fn a_spell_candidate_relates_to_its_own_targets_relative_to_the_chooser() {
    let engine = engine();
    // `instant spell that targets a creature you control`, chosen by the
    // controller of `attacker` (player-0).
    let filter = json!({"kind":"and","operands":[
        {"kind":"cardTypeContains","value":"Instant"},
        {"kind":"targets","permanents":{
            "where":{"kind":"cardTypeContains","value":"Creature"},
            "controller":"abilityController"
        }}
    ]});
    assert!(super::spell_candidate_filter_supported(&filter));
    // Spell filters read without the stack object keep refusing the relation.
    assert!(!super::super::stack_filter_supported(&filter));

    let at_creature = instant_on_stack(
        "player-1",
        vec![(
            "target",
            TargetRef::Permanent {
                instance_id: "attacker".to_string(),
            },
        )],
    );
    assert!(engine.spell_candidate_matches(&at_creature, &filter, "player-0"));
    // The same spell does not target a creature its opponent controls.
    assert!(!engine.spell_candidate_matches(&at_creature, &filter, "player-1"));

    let at_player = instant_on_stack(
        "player-1",
        vec![(
            "target",
            TargetRef::Player {
                player_id: "player-0".to_string(),
            },
        )],
    );
    assert!(!engine.spell_candidate_matches(&at_player, &filter, "player-0"));
    // `that targets you or a creature you control`.
    let you_or_creature = json!({"kind":"targets",
        "players":"abilityController",
        "permanents":{"where":{"kind":"cardTypeContains","value":"Creature"},
            "controller":"abilityController"}});
    assert!(engine.spell_candidate_matches(&at_player, &you_or_creature, "player-0"));
    assert!(!engine.spell_candidate_matches(&at_player, &you_or_creature, "player-1"));
    // A spell with no target never satisfies the relation.
    let untargeted = instant_on_stack("player-1", Vec::new());
    assert!(!engine.spell_candidate_matches(&untargeted, &you_or_creature, "player-0"));
}

#[test]
fn a_targets_relation_without_an_alternative_or_with_an_unknown_role_is_refused() {
    assert!(!super::spell_candidate_filter_supported(
        &json!({"kind":"targets"})
    ));
    assert!(!super::spell_candidate_filter_supported(
        &json!({"kind":"targets","players":"eventPlayer"})
    ));
    assert!(!super::spell_candidate_filter_supported(
        &json!({"kind":"targets",
        "permanents":{"where":null,"controller":"eventPlayer"}})
    ));
}
