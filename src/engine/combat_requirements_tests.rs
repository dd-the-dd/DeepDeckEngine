//! Runtime proofs of effect-created combat requirements, block capacity and
//! lone-combatant restrictions at declaration (CR 506.5, 508.1d, 509.1a-c,
//! 510.1d). Each test states the declaration that would become legal or
//! illegal without the rule.
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

fn self_static(modifier: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[modifier]})
}

fn marker(keyword: &str) -> Value {
    self_static(json!({"kind":"grantKeyword","objects":{"kind":"self"},"keyword":keyword}))
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
        508,
    )
    .unwrap()
}

/// Player 0 attacks player 1 with `attackers`; player 1 controls `blockers`.
fn attacking(engine: &mut GameEngine, attackers: &[&str]) {
    engine.state.combat.attackers = attackers
        .iter()
        .map(|id| DeclaredAttacker {
            attacker_id: (*id).into(),
            defender: TargetRef::Player {
                player_id: "player-1".into(),
            },
            defending_player_id: "player-1".into(),
        })
        .collect();
}

fn requirement(creature: &str, requirement: &str, field: Option<(&str, &str)>, turn: u32) -> Value {
    let mut modifier = json!({"kind":"combatRequirement","requirement":requirement,
        "creatureId":creature,"zoneIdentity":Value::Null,"expiresAfterTurn":turn,
        "untilEndOfCombat":false,"sourceCardInstanceId":"source"});
    if let Some((key, value)) = field {
        modifier[key] = json!(value);
    }
    modifier
}

fn block_pairs(actions: &[LegalAction]) -> Vec<(String, String)> {
    actions
        .iter()
        .filter(|action| action.kind == ActionKind::DeclareBlocker)
        .map(|action| {
            (
                action.blocker_id.clone().unwrap_or_default(),
                action.attacker_id.clone().unwrap_or_default(),
            )
        })
        .collect()
}

fn can_finish(actions: &[LegalAction], kind: ActionKind) -> bool {
    actions.iter().any(|action| action.kind == kind)
}

#[test]
fn a_directed_block_requirement_narrows_the_blocker_and_holds_the_declaration() {
    // `Target creature blocks this creature this turn if able.`: without the
    // requirement the blocker could block the other attacker or nothing.
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![
        instance("decoy", "player-0", "Creature", vec![]),
        instance("other", "player-0", "Creature", vec![]),
    ];
    engine.state.players[1].battlefield = vec![instance("forced", "player-1", "Creature", vec![])];
    attacking(&mut engine, &["decoy", "other"]);
    let turn = engine.state.turn_number;
    engine.state.rule_modifiers.push(requirement(
        "forced",
        "block",
        Some(("attackerId", "decoy")),
        turn,
    ));
    let attackers = vec!["decoy".to_string(), "other".to_string()];
    let actions = engine.legal_blocker_actions(1, &attackers, &BTreeSet::new(), &BTreeMap::new());
    assert_eq!(
        block_pairs(&actions),
        vec![("forced".to_string(), "decoy".to_string())]
    );
    assert!(
        !can_finish(&actions, ActionKind::FinishBlockers),
        "an obeyable requirement keeps the declaration open"
    );
    // Once obeyed, the declaration may end.
    let used = BTreeSet::from(["forced".to_string()]);
    let blocks = BTreeMap::from([("decoy".to_string(), vec!["forced".to_string()])]);
    let actions = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert!(can_finish(&actions, ActionKind::FinishBlockers));
    // An expired requirement (an earlier turn) no longer applies.
    engine.state.rule_modifiers = vec![requirement(
        "forced",
        "block",
        Some(("attackerId", "decoy")),
        turn.saturating_sub(1),
    )];
    if turn > 0 {
        let actions =
            engine.legal_blocker_actions(1, &attackers, &BTreeSet::new(), &BTreeMap::new());
        assert!(can_finish(&actions, ActionKind::FinishBlockers));
    }
}

#[test]
fn an_unobeyable_directed_requirement_excludes_nothing() {
    // The required attacker has flying: the requirement can't be obeyed, so
    // the blocker keeps its other choices (CR 509.1c) and may block nothing.
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![
        instance("flyer", "player-0", "Creature", vec![marker("flying")]),
        instance("other", "player-0", "Creature", vec![]),
    ];
    engine.state.players[1].battlefield = vec![instance("forced", "player-1", "Creature", vec![])];
    attacking(&mut engine, &["flyer", "other"]);
    let turn = engine.state.turn_number;
    engine.state.rule_modifiers.push(requirement(
        "forced",
        "block",
        Some(("attackerId", "flyer")),
        turn,
    ));
    let attackers = vec!["flyer".to_string(), "other".to_string()];
    let actions = engine.legal_blocker_actions(1, &attackers, &BTreeSet::new(), &BTreeMap::new());
    assert_eq!(
        block_pairs(&actions),
        vec![("forced".to_string(), "other".to_string())]
    );
    assert!(can_finish(&actions, ActionKind::FinishBlockers));
}

#[test]
fn lure_forces_every_able_blocker() {
    // `All creatures able to block target creature this turn do so.`
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![instance(
        "lure",
        "player-0",
        "Creature",
        vec![marker("allMustBlockIfAble")],
    )];
    engine.state.players[1].battlefield = vec![
        instance("first", "player-1", "Creature", vec![]),
        instance("second", "player-1", "Creature", vec![]),
    ];
    attacking(&mut engine, &["lure"]);
    let attackers = vec!["lure".to_string()];
    let used = BTreeSet::from(["first".to_string()]);
    let blocks = BTreeMap::from([("lure".to_string(), vec!["first".to_string()])]);
    let actions = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert!(
        !can_finish(&actions, ActionKind::FinishBlockers),
        "the second able blocker must block too"
    );
}

#[test]
fn block_capacity_offers_a_second_attacker_and_divides_damage_once() {
    // `This creature can block an additional creature each combat.`
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![
        instance("a", "player-0", "Creature", vec![]),
        instance("b", "player-0", "Creature", vec![]),
    ];
    let capacity = self_static(json!({"kind":"blockCapacity","objects":{"kind":"self"},
        "additional":1}));
    engine.state.players[1].battlefield =
        vec![instance("wide", "player-1", "Creature", vec![capacity])];
    attacking(&mut engine, &["a", "b"]);
    let attackers = vec!["a".to_string(), "b".to_string()];
    let used = BTreeSet::from(["wide".to_string()]);
    let blocks = BTreeMap::from([("a".to_string(), vec!["wide".to_string()])]);
    let actions = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert_eq!(
        block_pairs(&actions),
        vec![("wide".to_string(), "b".to_string())],
        "one more attacker, never the one it already blocks"
    );
    // Capacity exhausted after the second block.
    let blocks = BTreeMap::from([
        ("a".to_string(), vec!["wide".to_string()]),
        ("b".to_string(), vec!["wide".to_string()]),
    ]);
    let actions = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert!(block_pairs(&actions).is_empty());
    // CR 510.1d: its damage is dealt once, not to each attacker.
    engine.state.combat.blockers = vec![
        DeclaredBlocker {
            attacker_id: "a".into(),
            blocker_id: "wide".into(),
        },
        DeclaredBlocker {
            attacker_id: "b".into(),
            blocker_id: "wide".into(),
        },
    ];
    assert_eq!(engine.blocker_damage_share("wide", "a", 2, &blocks), 2);
    assert_eq!(engine.blocker_damage_share("wide", "b", 2, &blocks), 0);
    // Without the capacity a used blocker is not offered again.
    engine.state.players[1].battlefield = vec![instance("wide", "player-1", "Creature", vec![])];
    let blocks = BTreeMap::from([("a".to_string(), vec!["wide".to_string()])]);
    let actions = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert!(block_pairs(&actions).is_empty());
}

#[test]
fn a_creature_that_cant_attack_alone_needs_a_companion() {
    // `This creature can't attack alone.`
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![instance(
        "flunky",
        "player-0",
        "Creature",
        vec![marker("cantAttackAlone")],
    )];
    let offered = |engine: &GameEngine, selected: &BTreeSet<String>, id: &str| {
        engine
            .legal_attacker_actions(selected)
            .iter()
            .any(|action| {
                action.kind == ActionKind::DeclareAttacker
                    && action.attacker_id.as_deref() == Some(id)
            })
    };
    assert!(
        !offered(&engine, &BTreeSet::new(), "flunky"),
        "no companion"
    );
    engine.state.players[0]
        .battlefield
        .push(instance("friend", "player-0", "Creature", vec![]));
    assert!(offered(&engine, &BTreeSet::new(), "flunky"));
    // Declared alone, the declaration can't end while the friend can join.
    let alone = BTreeSet::from(["flunky".to_string()]);
    let actions = engine.legal_attacker_actions(&alone);
    assert!(!can_finish(&actions, ActionKind::FinishAttackers));
    let together = BTreeSet::from(["flunky".to_string(), "friend".to_string()]);
    let actions = engine.legal_attacker_actions(&together);
    assert!(can_finish(&actions, ActionKind::FinishAttackers));
}

#[test]
fn a_directed_attack_requirement_names_the_player_not_their_planeswalkers() {
    // `Target creature attacks target opponent this turn if able.`
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![instance("raider", "player-0", "Creature", vec![])];
    engine.state.players[1].battlefield = vec![instance(
        "walker",
        "player-1",
        "Legendary Planeswalker — Jace",
        vec![],
    )];
    let turn = engine.state.turn_number;
    let defenders = |engine: &GameEngine| {
        engine
            .legal_attacker_actions(&BTreeSet::new())
            .into_iter()
            .filter(|action| action.attacker_id.as_deref() == Some("raider"))
            .filter_map(|action| action.targets.get("defender").cloned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        defenders(&engine).len(),
        2,
        "the player and the planeswalker"
    );
    engine.state.rule_modifiers.push(requirement(
        "raider",
        "attack",
        Some(("defenderPlayerId", "player-1")),
        turn,
    ));
    assert_eq!(
        defenders(&engine),
        vec![TargetRef::Player {
            player_id: "player-1".into()
        }]
    );
    assert!(
        !can_finish(
            &engine.legal_attacker_actions(&BTreeSet::new()),
            ActionKind::FinishAttackers
        ),
        "the requirement is obeyable"
    );
}

#[test]
fn a_combat_window_requirement_ends_with_the_combat() {
    let mut engine = engine();
    let turn = engine.state.turn_number;
    let mut combat_only = requirement("raider", "attack", None, turn);
    combat_only["untilEndOfCombat"] = json!(true);
    let turn_long = requirement("other", "attack", None, turn);
    assert!(super::ends_with_combat(&combat_only));
    assert!(!super::ends_with_combat(&turn_long));
    engine.state.rule_modifiers = vec![combat_only, turn_long.clone()];
    engine.clear_combat_state();
    assert_eq!(engine.state.rule_modifiers, vec![turn_long]);
}

#[test]
fn requirement_effects_validate_their_direction_and_window() {
    let effect = |extra: Value| {
        let mut effect = json!({"kind":"installCombatRequirement","requirement":"block",
            "creatures":{"kind":"chosenTarget","id":"target"},
            "window":{"kind":"untilEndOfCurrentTurn"}});
        for (key, value) in extra.as_object().unwrap() {
            effect[key] = value.clone();
        }
        effect
    };
    assert!(super::combat_requirement_effect_supported(&effect(json!({
        "attacker":{"kind":"self"}
    }))));
    assert!(super::combat_requirement_effect_supported(&effect(json!({
        "window":{"kind":"thisCombat"}
    }))));
    // A block requirement names no defender; a standing window is refused.
    assert!(!super::combat_requirement_effect_supported(&effect(
        json!({
            "defender":{"kind":"abilityController"}
        })
    )));
    assert!(!super::combat_requirement_effect_supported(&effect(
        json!({
            "window":{"kind":"permanent"}
        })
    )));
}
