//! Real Oracle text -> primitive IR -> engine behavior for control changes
//! (CR 613.1b, 701.12), stack copies (CR 707.10) and one sentence acting for
//! several players (CR 608.2c).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn last_rule(name: &str, type_line: &str, oracle: &str) -> Value {
    parsed_rules(name, type_line, oracle)
        .pop()
        .expect("a canonical rule")
}

fn stack_object(rule: &Value, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:control-copy-players".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("source", test_definition("source", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn run(engine: &mut GameEngine, rule: &Value, targets: BTreeMap<String, TargetRef>) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &stack_object(rule, targets),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed program resolves");
}

fn player_target(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn permanent_target(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn controller_of(engine: &GameEngine, instance_id: &str) -> String {
    let (player, index) = engine
        .permanent_position(instance_id)
        .expect("the permanent is on the battlefield");
    engine.state.players[player].battlefield[index]
        .controller
        .clone()
}

#[test]
fn parsed_you_and_target_opponent_each_draw() {
    let rule = last_rule(
        "Secret Rendezvous",
        "Sorcery",
        "You and target opponent each draw three cards.",
    );
    let mut engine = test_engine(3);
    let before = engine
        .state
        .players
        .iter()
        .map(|player| player.hand.len())
        .collect::<Vec<_>>();
    run(
        &mut engine,
        &rule,
        BTreeMap::from([("targetPlayer".to_string(), player_target("player-2"))]),
    );
    let after = engine
        .state
        .players
        .iter()
        .map(|player| player.hand.len())
        .collect::<Vec<_>>();
    assert_eq!(after[0], before[0] + 3);
    assert_eq!(after[1], before[1], "an untargeted opponent draws nothing");
    assert_eq!(after[2], before[2] + 3);
}

#[test]
fn parsed_each_player_draws_then_discards() {
    let rule = last_rule(
        "Geier Reach Sanitarium",
        "Legendary Land",
        "{2}, {T}: Each player draws a card, then discards a card.",
    );
    let mut engine = test_engine(2);
    let hands = engine
        .state
        .players
        .iter()
        .map(|player| player.hand.len())
        .collect::<Vec<_>>();
    let graveyards = engine
        .state
        .players
        .iter()
        .map(|player| player.graveyard.len())
        .collect::<Vec<_>>();
    run(&mut engine, &rule, BTreeMap::new());
    for (index, player) in engine.state.players.iter().enumerate() {
        assert_eq!(player.hand.len(), hands[index]);
        assert_eq!(player.graveyard.len(), graveyards[index] + 1);
    }
}

fn creature(id: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, "Creature — Bear"), owner)
}

#[test]
fn parsed_gain_control_until_end_of_turn_and_indefinitely() {
    let temporary = last_rule(
        "Probe",
        "Sorcery",
        "Until end of turn, you gain control of target creature and it gains haste.",
    );
    let target_id = temporary["declaration"]["decisions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![creature("bear", "player-1")];
    run(
        &mut engine,
        &temporary,
        BTreeMap::from([(target_id, permanent_target("bear"))]),
    );
    assert_eq!(controller_of(&engine, "bear"), "player-0");
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("controlUntilEndOfTurn")
            && modifier["cardInstanceId"] == "bear"
    }));

    let indefinite = last_rule(
        "Empress Galina",
        "Legendary Creature — Merfolk Noble",
        "{U}{U}, {T}: Gain control of target legendary permanent. (This effect lasts indefinitely.)",
    );
    let target_id = indefinite["declaration"]["decisions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut engine = test_engine(2);
    let mut legend = test_definition("legend", "Legendary Artifact");
    legend.name = "Legend".to_string();
    engine.state.players[1].battlefield = vec![test_instance("legend", legend, "player-1")];
    run(
        &mut engine,
        &indefinite,
        BTreeMap::from([(target_id, permanent_target("legend"))]),
    );
    assert_eq!(controller_of(&engine, "legend"), "player-0");
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some("controlUntilEndOfTurn")),
        "an indefinite control change installs no expiry"
    );
}

#[test]
fn parsed_exchange_control_swaps_two_targets() {
    let rule = last_rule(
        "Switcheroo",
        "Sorcery",
        "Exchange control of two target creatures.",
    );
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("mine", "player-0")];
    engine.state.players[1].battlefield = vec![creature("theirs", "player-1")];
    run(
        &mut engine,
        &rule,
        BTreeMap::from([
            (format!("{target_id}:0"), permanent_target("mine")),
            (format!("{target_id}:1"), permanent_target("theirs")),
        ]),
    );
    assert_eq!(controller_of(&engine, "mine"), "player-1");
    assert_eq!(controller_of(&engine, "theirs"), "player-0");
}

#[test]
fn parsed_control_magic_controls_the_enchanted_creature_while_attached() {
    let rules = parsed_rules(
        "Control Magic",
        "Enchantment — Aura",
        "Enchant creature\nYou control enchanted creature.",
    );
    let mut aura_definition = test_definition("aura", "Enchantment — Aura");
    aura_definition.rules = rules;
    let mut aura = test_instance("aura", aura_definition, "player-0");
    aura.attached_to = Some("bear".to_string());
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![aura];
    engine.state.players[1].battlefield = vec![creature("bear", "player-1")];
    engine.refresh_control_effects();
    assert_eq!(controller_of(&engine, "bear"), "player-0");
}

#[test]
fn parsed_copy_target_spell_puts_a_copy_controlled_by_the_copier() {
    let rule = last_rule(
        "Dualcaster Mage",
        "Creature — Human Wizard",
        "Flash\nWhen this creature enters, copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    let mut engine = test_engine(2);
    let spell = test_instance("bolt", test_definition("bolt", "Instant"), "player-1");
    engine.state.stack.push(StackObject {
        id: "stack:bolt".to_string(),
        controller: "player-1".to_string(),
        card: spell,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    let mut trigger = stack_object(&rule, BTreeMap::new());
    trigger.targets = BTreeMap::from([(
        "targetSpell".to_string(),
        TargetRef::StackObject {
            stack_id: "stack:bolt".to_string(),
        },
    )]);
    engine
        .execute_effects(
            &rule["effects"],
            &trigger,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the copy resolves");
    assert_eq!(engine.state.stack.len(), 2);
    let copy = engine.state.stack.last().unwrap();
    assert_eq!(
        copy.controller, "player-0",
        "CR 707.10: the copier controls it"
    );
    assert_eq!(copy.card.flags.get("isSpellCopy"), Some(&true));
}

#[test]
fn parsed_addendum_bonus_belongs_to_the_cast_spell_not_its_copy() {
    // CR 707.10: a copy copies the spell's choices but is not cast, so a
    // cast-history condition such as Addendum is false for the copy.
    let spell_rule = last_rule(
        "Sphinx's Insight",
        "Instant",
        "Draw two cards.\nAddendum — If you cast this spell during your main phase, you gain 2 life.",
    );
    let copier = last_rule(
        "Dualcaster Mage",
        "Creature — Human Wizard",
        "Flash\nWhen this creature enters, copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].library = (0..6)
        .map(|index| {
            test_instance(
                &format!("library-{index}"),
                test_definition("Card", "Instant"),
                "player-0",
            )
        })
        .collect();
    let mut spell = test_instance(
        "insight",
        test_definition("Sphinx's Insight", "Instant"),
        "player-0",
    );
    for flag in ["wasCast", "wasCastFromHand", "wasCastDuringMainPhase"] {
        spell.flags.insert(flag.to_string(), true);
    }
    let original = StackObject {
        id: "stack:insight".to_string(),
        controller: "player-0".to_string(),
        card: spell,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(spell_rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine.state.stack.push(original.clone());
    let mut trigger = stack_object(&copier, BTreeMap::new());
    trigger.targets = BTreeMap::from([(
        "targetSpell".to_string(),
        TargetRef::StackObject {
            stack_id: "stack:insight".to_string(),
        },
    )]);
    engine
        .execute_effects(
            &copier["effects"],
            &trigger,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the copy resolves");
    let copy = engine.state.stack.pop().expect("the copy is on the stack");
    for flag in ["wasCast", "wasCastFromHand", "wasCastDuringMainPhase"] {
        assert_eq!(copy.card.flags.get(flag), None, "{flag} is not copied");
    }
    let life = engine.state.players[0].life;
    for (object, bonus) in [(&copy, 0), (&original, 2)] {
        let before = engine.state.players[0].life;
        execute_effects_at_resolution_start(
            &mut engine,
            &spell_rule["effects"],
            object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the spell resolves");
        assert_eq!(
            engine.state.players[0].life - before,
            bonus,
            "{}",
            object.id
        );
    }
    assert_eq!(engine.state.players[0].life, life + 2);
}
