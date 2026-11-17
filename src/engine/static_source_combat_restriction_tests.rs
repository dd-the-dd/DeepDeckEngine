use super::*;

fn definition(id: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":"Creature",
        "power":"2","toughness":"2","rules":rules}))
    .unwrap()
}

fn instance(id: &str, owner: &str, rules: Vec<Value>) -> CardInstance {
    let mut card = card_instance(owner, &definition(id, rules), 0);
    card.instance_id = id.into();
    card.summoning_sick = false;
    card
}

fn restriction(marker: &str) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"grantKeyword","objects":{"kind":"self"},"keyword":marker}]})
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", vec![]); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2711,
    )
    .unwrap()
}

#[test]
fn source_block_prohibition_changes_real_blocker_options_and_is_an_ability() {
    let mut engine = engine();
    engine.state.active_player = 1;
    engine.state.players[0].battlefield = vec![instance(
        "restricted",
        "player-0",
        vec![restriction("cantBlock")],
    )];
    engine.state.players[1].battlefield = vec![instance("attacker", "player-1", vec![])];
    let actions =
        engine.legal_blocker_actions(0, &["attacker".into()], &BTreeSet::new(), &BTreeMap::new());
    assert!(
        !actions.iter().any(|a| a.kind == ActionKind::DeclareBlocker
            && a.blocker_id.as_deref() == Some("restricted"))
    );
    let mut suppression = restriction("cantBlock");
    suppression["modifiers"] = json!([{"kind":"loseAllAbilities","objects":{"kind":"permanents","where":{"kind":"nameEquals","value":"restricted"}}}]);
    engine.state.players[0]
        .battlefield
        .push(instance("suppressor", "player-0", vec![suppression]));
    let actions =
        engine.legal_blocker_actions(0, &["attacker".into()], &BTreeSet::new(), &BTreeMap::new());
    assert!(
        actions.iter().any(|a| a.kind == ActionKind::DeclareBlocker
            && a.blocker_id.as_deref() == Some("restricted"))
    );
}

#[test]
fn attack_prohibition_and_unblockability_reach_their_separate_combat_services() {
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![
        instance("restricted", "player-0", vec![restriction("cantAttack")]),
        instance("plain", "player-0", vec![]),
    ];
    let actions = engine.legal_attacker_actions(&BTreeSet::new());
    assert!(
        !actions.iter().any(|a| a.kind == ActionKind::DeclareAttacker
            && a.attacker_id.as_deref() == Some("restricted"))
    );
    assert!(actions.iter().any(
        |a| a.kind == ActionKind::DeclareAttacker && a.attacker_id.as_deref() == Some("plain")
    ));
    assert!(!engine.permanent_has_keyword(&engine.state.players[0].battlefield[0], "defender"));
    engine.state.active_player = 1;
    engine.state.players[1].battlefield = vec![instance(
        "unblockable",
        "player-1",
        vec![restriction("cantBeBlocked")],
    )];
    let actions = engine.legal_blocker_actions(
        0,
        &["unblockable".into()],
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    assert!(!actions.iter().any(|a| a.kind == ActionKind::DeclareBlocker));
}
