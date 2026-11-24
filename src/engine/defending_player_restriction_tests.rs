//! Runtime proofs of static combat restrictions gated on the defending
//! player's state (CR 506.2, 508.1c, 509.1b): the condition is evaluated for
//! the defending player of each attack or block check, never for the
//! restriction's controller and never as a standing gate.
use super::super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,
        "power":"2","toughness":"2","rules":rules}))
    .unwrap()
}

fn instance(id: &str, owner: &str, type_line: &str, rules: Vec<Value>) -> CardInstance {
    let mut card = card_instance(owner, &definition(id, type_line, rules), 0);
    card.instance_id = id.into();
    card.summoning_sick = false;
    card
}

/// `defending player controls a PERMANENT with SUBTYPE`, as the compiler
/// lowers it for a static combat restriction.
fn defending_player_controls(subtype: &str) -> Value {
    json!({"kind":"compare","operator":">=",
        "left":{"kind":"countPermanents","player":{"kind":"defendingPlayer"},
            "where":{"kind":"subtypeContains","value":subtype},"excludeSource":false},
        "right":{"kind":"integer","value":1}})
}

fn per_combat_restriction(restriction: &str, condition: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"defendingPlayerCombatRestriction","objects":{"kind":"self"},
            "restriction":restriction,"condition":condition}]})
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Creature", vec![]); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        506,
    )
    .unwrap()
}

fn can_declare_attacker(engine: &GameEngine, attacker: &str) -> bool {
    engine
        .legal_attacker_actions(&BTreeSet::new())
        .iter()
        .any(|action| {
            action.kind == ActionKind::DeclareAttacker
                && action.attacker_id.as_deref() == Some(attacker)
        })
}

#[test]
fn cant_attack_unless_reads_the_defending_players_islands_not_the_controllers() {
    // `This creature can't attack unless defending player controls an Island.`
    let mut engine = engine();
    let restricted = per_combat_restriction(
        "cantAttack",
        json!({"kind":"not","operand":defending_player_controls("Island")}),
    );
    engine.state.players[0].battlefield = vec![
        instance("serpent", "player-0", "Creature", vec![restricted]),
        // The restriction's controller owns an Island: it must not count.
        instance("own-island", "player-0", "Land — Island", vec![]),
    ];
    assert!(
        !can_declare_attacker(&engine, "serpent"),
        "the defending player controls no Island"
    );
    engine.state.players[1].battlefield.push(instance(
        "their-island",
        "player-1",
        "Land — Island",
        vec![],
    ));
    assert!(
        can_declare_attacker(&engine, "serpent"),
        "the defending player now controls an Island"
    );
    // The modifier is no standing keyword: nothing else reads it as one.
    assert!(!engine.permanent_has_keyword(&engine.state.players[0].battlefield[0], "cantAttack"));
}

#[test]
fn cant_be_blocked_as_long_as_reads_the_attackers_defending_player() {
    // `This creature can't be blocked as long as defending player controls
    // an artifact.` (the subtype stands for the controlled-permanent query).
    let mut engine = engine();
    engine.state.active_player = 1;
    engine.state.players[1].battlefield = vec![instance(
        "beebles",
        "player-1",
        "Creature",
        vec![per_combat_restriction(
            "cantBeBlocked",
            defending_player_controls("Equipment"),
        )],
    )];
    engine.state.players[0].battlefield = vec![instance("wall", "player-0", "Creature", vec![])];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "beebles".into(),
        defender: TargetRef::Player {
            player_id: "player-0".into(),
        },
        defending_player_id: "player-0".into(),
    }];
    let blockers = |engine: &GameEngine| {
        engine
            .legal_blocker_actions(0, &["beebles".into()], &BTreeSet::new(), &BTreeMap::new())
            .into_iter()
            .filter(|action| action.kind == ActionKind::DeclareBlocker)
            .count()
    };
    assert!(
        blockers(&engine) > 0,
        "the condition is false: it can be blocked"
    );
    engine.state.players[0].battlefield.push(instance(
        "sword",
        "player-0",
        "Artifact — Equipment",
        vec![],
    ));
    assert_eq!(blockers(&engine), 0, "the defending player controls one");
}

#[test]
fn a_per_combat_restriction_without_a_defending_player_read_is_not_executable() {
    let restricted = per_combat_restriction(
        "cantAttack",
        json!({"kind":"compare","operator":">=",
            "left":{"kind":"countPermanents","player":{"kind":"abilityController"},
                "where":{"kind":"subtypeContains","value":"Island"},"excludeSource":false},
            "right":{"kind":"integer","value":1}}),
    );
    assert!(!defending_player_restriction::modifier_supported(
        &restricted["modifiers"][0]
    ));
    let supported = per_combat_restriction(
        "cantAttack",
        json!({"kind":"not","operand":defending_player_controls("Island")}),
    );
    assert!(defending_player_restriction::modifier_supported(
        &supported["modifiers"][0]
    ));
}
