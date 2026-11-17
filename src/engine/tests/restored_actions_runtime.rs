//! Runtime proofs for primitives restored with the dungeon rooms: goad over a
//! declared target (CR 701.38a), a named legendary token (CR 111.4, 205.4d),
//! and a distributed `each player ... unless they pay` frame (CR 118.12).
use super::*;

fn parsed_rules(type_line: &str, text: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Restored Action Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("restored-actions-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect::<Vec<_>>();
    assert!(rules.iter().all(rule_is_executable), "{rules:#?}");
    rules
}

fn room_stack_object(source: &CardInstance, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:restored-room".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("dungeonRoom".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets,
    }
}

#[test]
fn goad_room_designates_the_declared_creature_for_the_goading_player() {
    let rules = parsed_rules(
        "Dungeon",
        "Arena \u{2014} Goad target creature. (Leads to: Archives)\nArchives \u{2014} Draw a card.",
    );
    let mut engine = test_engine(2);
    let creature = test_instance(
        "goad-target",
        test_definition("goad-target", "Creature"),
        "player-1",
    );
    engine.state.players[1].battlefield.push(creature);
    let dungeon = test_instance("dungeon", test_definition("dungeon", "Dungeon"), "player-0");
    let stack_object = room_stack_object(
        &dungeon,
        BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "goad-target".to_string(),
            },
        )]),
    );
    engine
        .execute_effects(
            &rules[0]["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the goad room resolves");
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("goaded") && modifier["goadingPlayerId"] == "player-0"
    }));
}

#[test]
fn named_legendary_token_keeps_its_name_and_supertype() {
    let rules = parsed_rules(
        "Sorcery",
        "Create The Atropal, a legendary 4/4 black God Horror creature token with deathtouch.",
    );
    let mut engine = test_engine(2);
    let source = test_instance(
        "token-source",
        test_definition("token-source", "Sorcery"),
        "player-0",
    );
    let stack_object = room_stack_object(&source, BTreeMap::new());
    engine
        .execute_effects(
            &rules[0]["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the token is created");
    let token = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.is_token)
        .expect("a token on the battlefield");
    assert_eq!(token.definition.name, "The Atropal");
    assert!(token.definition.type_line.starts_with("Legendary Creature"));
}

#[test]
fn each_player_who_cannot_pay_suffers_the_guarded_loss() {
    let rules = parsed_rules(
        "Sorcery",
        "Each player loses 2 life unless they discard a card.",
    );
    let mut engine = test_engine(2);
    for player in &mut engine.state.players {
        player.hand.clear();
    }
    let source = test_instance("veils", test_definition("veils", "Sorcery"), "player-0");
    let stack_object = room_stack_object(&source, BTreeMap::new());
    let before = engine
        .state
        .players
        .iter()
        .map(|player| player.life)
        .collect::<Vec<_>>();
    engine
        .execute_effects(
            &rules[0]["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the distributed payment resolves");
    for (player, life) in engine.state.players.iter().zip(before) {
        assert_eq!(player.life, life - 2, "an empty hand cannot pay (CR 118.3)");
    }
}
