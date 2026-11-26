//! Real Oracle text -> trigger event language IR -> engine behavior: player
//! events read through the verb lexicon (CR 724.1 monarch, 701.23 library
//! search, 707.10 spell copy, 122.1 energy), a player member of the targeted
//! subject (CR 115.1), an attacked player restricted by the shared player
//! relative predicate (CR 506.2) and a quantified permanent performing a
//! keyword action on another (CR 702.110b exploit).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
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
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn parsed_card(
    instance_id: &str,
    owner: &str,
    name: &str,
    type_line: &str,
    oracle: &str,
) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.rules = parsed_rules(name, type_line, oracle);
    test_instance(instance_id, definition, owner)
}

/// The executable definition of a real card whose keyword line expands
/// (`Exploit`), through the playable-card adapter.
fn compiled_card(instance_id: &str, owner: &str, name: &str, oracle: &str) -> CardInstance {
    let compilation = compile_playable_card(PlayableCardInput {
        id: instance_id.to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: "Creature \u{2014} Zombie".to_string(),
            mana_cost: Some("{2}{B}".to_string()),
            oracle_text: Some(oracle.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{name}: {:#?}",
        compilation.parser_result.diagnostics
    );
    test_instance(instance_id, compilation.card, owner)
}

fn plain_card(instance_id: &str, owner: &str, type_line: &str) -> CardInstance {
    test_instance(instance_id, test_definition(instance_id, type_line), owner)
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn triggers_from(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| {
            event.kind == "triggeredAbilityPutOnStack"
                && event.card_instance_id.as_deref() == Some(instance_id)
        })
        .count()
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the stack resolves");
    }
}

/// A resolving spell of `controller` whose effects are `oracle`.
fn resolve_spell_effects(engine: &mut GameEngine, controller: &str, oracle: &str) {
    let spell = parsed_rules("Effect Source", "Sorcery", oracle)
        .into_iter()
        .next()
        .expect("the spell compiles");
    let source = StackObject {
        id: format!("stack:{controller}:effect"),
        controller: controller.to_string(),
        card: plain_card(&format!("{controller}-effect"), controller, "Sorcery"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        engine,
        &spell["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell's effects resolve");
}

/// A spell of `controller` on the stack with the given targets.
fn targeting_spell(
    engine: &mut GameEngine,
    controller: &str,
    stack_id: &str,
    type_line: &str,
    targets: &BTreeMap<String, TargetRef>,
) {
    engine.state.stack.push(StackObject {
        id: stack_id.to_string(),
        controller: controller.to_string(),
        card: plain_card(stack_id, controller, type_line),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: targets.clone(),
    });
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn becoming_the_monarch_triggers_the_written_player_relation() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "lich",
            "player-0",
            "Custodi Lich",
            "Creature \u{2014} Zombie Cleric",
            "When this creature enters, you become the monarch.\nWhenever you become the monarch, target player sacrifices a creature of their choice.",
        ),
        // Unused combination: an opponent as the new monarch.
        parsed_card(
            "usurped",
            "player-0",
            "Usurped Crown Probe",
            "Enchantment",
            "Whenever an opponent becomes the monarch, you gain 2 life.",
        ),
    ];
    admit(&mut engine);
    engine.become_monarch("player-0");
    assert_eq!(triggers_from(&engine, "lich"), 1, "you became the monarch");
    assert_eq!(triggers_from(&engine, "usurped"), 0);
    engine.state.stack.clear();
    engine.become_monarch("player-1");
    assert_eq!(triggers_from(&engine, "lich"), 1, "the opponent is not you");
    assert_eq!(
        triggers_from(&engine, "usurped"),
        1,
        "an opponent became it"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn ob_nixilis_punishes_an_opponents_fetch_only() {
    const OB_NIXILIS: &str = "Flying, trample\nWhenever an opponent searches their library, that player sacrifices a creature of their choice and loses 10 life.\nWhenever another creature dies, put a +1/+1 counter on Ob Nixilis.";
    const FETCH: &str = "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
    for (searcher, triggers) in [("player-0", 0), ("player-1", 1)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![parsed_card(
            "ob",
            "player-0",
            "Ob Nixilis, Unshackled",
            "Legendary Creature \u{2014} Demon",
            OB_NIXILIS,
        )];
        engine.state.players[1].battlefield = vec![plain_card(
            "their-bear",
            "player-1",
            "Creature \u{2014} Bear",
        )];
        admit(&mut engine);
        resolve_spell_effects(&mut engine, searcher, FETCH);
        assert_eq!(
            triggers_from(&engine, "ob"),
            triggers,
            "{searcher} searched"
        );
        resolve_stack(&mut engine);
        assert_eq!(
            engine.state.players[1].life,
            20 - 10 * triggers as i32,
            "that player is the searching opponent"
        );
        assert_eq!(engine.state.players[0].life, 20);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn getting_energy_is_one_occurrence_per_instruction_of_the_related_player() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "module",
            "player-0",
            "Fabrication Module",
            "Artifact",
            "Whenever you get one or more {E} (energy counters), put a +1/+1 counter on target creature you control.\n{4}, {T}: You get {E}.",
        ),
        // Unused combination: an opponent getting energy.
        parsed_card(
            "watcher",
            "player-0",
            "Energy Watcher Probe",
            "Enchantment",
            "Whenever an opponent gets one or more {E}, you gain 1 life.",
        ),
        plain_card("bear", "player-0", "Creature \u{2014} Bear"),
    ];
    admit(&mut engine);
    resolve_spell_effects(&mut engine, "player-0", "You get {E}{E}.");
    assert_eq!(
        triggers_from(&engine, "module"),
        1,
        "two {{E}}, one instruction"
    );
    assert_eq!(triggers_from(&engine, "watcher"), 0);
    resolve_spell_effects(&mut engine, "player-1", "You get {E}.");
    assert_eq!(triggers_from(&engine, "module"), 1, "not your energy");
    assert_eq!(triggers_from(&engine, "watcher"), 1, "an opponent's energy");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn copying_a_spell_triggers_on_the_copied_spell_and_its_controller() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "kalamax",
            "player-0",
            "Kalamax, the Stormsire",
            "Legendary Creature \u{2014} Elemental Dinosaur",
            "Whenever you cast your first instant spell each turn, if Kalamax is tapped, copy that spell. You may choose new targets for the copy.\nWhenever you copy an instant spell, put a +1/+1 counter on Kalamax.",
        ),
        // Unused combination: an opponent copying a sorcery.
        parsed_card(
            "echo",
            "player-0",
            "Echo Watcher Probe",
            "Enchantment",
            "Whenever an opponent copies a sorcery spell, you gain 2 life.",
        ),
    ];
    admit(&mut engine);
    targeting_spell(
        &mut engine,
        "player-0",
        "stack:bolt",
        "Instant",
        &BTreeMap::new(),
    );
    engine
        .copy_stack_object("stack:bolt")
        .expect("the instant is copied");
    assert_eq!(
        triggers_from(&engine, "kalamax"),
        1,
        "you copied an instant"
    );
    assert_eq!(triggers_from(&engine, "echo"), 0);
    targeting_spell(
        &mut engine,
        "player-1",
        "stack:rite",
        "Sorcery",
        &BTreeMap::new(),
    );
    engine
        .copy_stack_object("stack:rite")
        .expect("the sorcery is copied");
    assert_eq!(triggers_from(&engine, "kalamax"), 1, "not your copy");
    assert_eq!(
        triggers_from(&engine, "echo"),
        1,
        "an opponent copied a sorcery"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_targeted_player_member_triggers_once_per_targeting_object() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "leovold",
            "player-0",
            "Leovold, Emissary of Trest",
            "Legendary Creature \u{2014} Elf Advisor",
            "Each opponent can't draw more than one card each turn.\nWhenever you or a permanent you control becomes the target of a spell or ability an opponent controls, you may draw a card.",
        ),
        parsed_card(
            "gomazoa",
            "player-0",
            "Dormant Gomazoa",
            "Creature \u{2014} Jellyfish",
            "Flying\nThis creature enters tapped.\nThis creature doesn't untap during your untap step.\nWhenever you become the target of a spell, you may untap this creature.",
        ),
        plain_card("own-bear", "player-0", "Creature \u{2014} Bear"),
    ];
    admit(&mut engine);
    let player = |id: &str| TargetRef::Player {
        player_id: id.to_string(),
    };
    // The opponent's spell targets you and a permanent you control: one
    // occurrence for Leovold, one for the player member alone.
    let both = BTreeMap::from([
        ("player".to_string(), player("player-0")),
        (
            "creature".to_string(),
            TargetRef::Permanent {
                instance_id: "own-bear".to_string(),
            },
        ),
    ]);
    targeting_spell(&mut engine, "player-1", "stack:both", "Instant", &both);
    engine.enqueue_generic_became_target_triggers("stack:both", &both);
    assert_eq!(
        triggers_from(&engine, "leovold"),
        1,
        "once per targeting spell"
    );
    assert_eq!(triggers_from(&engine, "gomazoa"), 1, "you became a target");
    // Your own spell targeting you: not an opponent's (Leovold), still a
    // spell (Gomazoa).
    let you = BTreeMap::from([("player".to_string(), player("player-0"))]);
    targeting_spell(&mut engine, "player-0", "stack:own", "Sorcery", &you);
    engine.enqueue_generic_became_target_triggers("stack:own", &you);
    assert_eq!(triggers_from(&engine, "leovold"), 1);
    assert_eq!(triggers_from(&engine, "gomazoa"), 2);
    // The opponent targets only themselves.
    let them = BTreeMap::from([("player".to_string(), player("player-1"))]);
    targeting_spell(&mut engine, "player-1", "stack:them", "Instant", &them);
    engine.enqueue_generic_became_target_triggers("stack:them", &them);
    assert_eq!(triggers_from(&engine, "leovold"), 1);
    assert_eq!(triggers_from(&engine, "gomazoa"), 2);
}

/// The attack declaration as the combat step performs it, against
/// `defending_player_id`.
fn declare_attack(engine: &mut GameEngine, attacker_id: &str, defending_player_id: &str) {
    let active = engine.state.players[engine.state.active_player].id.clone();
    let (player_index, card_index) = engine.permanent_position(attacker_id).expect("attacker");
    engine.state.players[player_index].battlefield[card_index]
        .flags
        .insert("attacking".to_string(), true);
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker_id.to_string(),
        defender: TargetRef::Player {
            player_id: defending_player_id.to_string(),
        },
        defending_player_id: defending_player_id.to_string(),
    });
    engine.record_event(
        "attackerDeclared",
        Some(active.clone()),
        Some(attacker_id.to_string()),
        json!({
            "defendingPlayerId": defending_player_id,
            "objectIdentity": engine.object_identity_value(attacker_id),
        }),
    );
    engine.record_event(
        "attackDeclared",
        Some(active),
        None,
        json!({"attackerIds": [attacker_id]}),
    );
    engine.enqueue_declared_attacker_triggers(&[attacker_id.to_string()]);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_attacked_player_relative_is_read_on_the_defending_player() {
    for (lands, triggers) in [(7, 0), (8, 1)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![parsed_card(
            "owlbear",
            "player-0",
            "Owlbear Cub",
            "Creature \u{2014} Bird Bear",
            "Whenever this creature attacks a player who controls eight or more lands, you gain 3 life.",
        )];
        engine.state.players[1].battlefield = (0..lands)
            .map(|index| {
                plain_card(
                    &format!("their-land-{index}"),
                    "player-1",
                    "Basic Land \u{2014} Forest",
                )
            })
            .collect();
        admit(&mut engine);
        declare_attack(&mut engine, "owlbear", "player-1");
        assert_eq!(
            triggers_from(&engine, "owlbear"),
            triggers,
            "the attacked player controls {lands} lands"
        );
    }
}

/// Sacrifices the `fodder` creature for exploit and accepts every optional
/// choice; any other choice is the first option.
struct ExploitFodder;

impl DecisionProvider for ExploitFodder {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(index) = request.options.iter().position(|option| {
            serde_json::to_string(option).is_ok_and(|text| text.contains("\"fodder\""))
        }) {
            return Ok(index);
        }
        Ok(match request.kind {
            DecisionKind::ResolutionChoice => request.options.len().saturating_sub(1),
            _ => 0,
        })
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn skull_skaab_observes_a_creature_you_control_exploiting_a_nontoken_creature() {
    const SKULL_SKAAB: &str = "Exploit (When this creature enters, you may sacrifice a creature.)\nWhenever a creature you control exploits a nontoken creature, create a 2/2 black Zombie creature token.";
    const EXPLOITER: &str = "Exploit (When this creature enters, you may sacrifice a creature.)";
    for (fodder_is_token, zombies) in [(false, 1), (true, 0)] {
        let mut engine = test_engine(2);
        let mut fodder = plain_card("fodder", "player-0", "Creature \u{2014} Human");
        fodder.definition.is_token = fodder_is_token;
        engine.state.players[0].battlefield = vec![
            compiled_card("skaab", "player-0", "Skull Skaab", SKULL_SKAAB),
            fodder,
        ];
        admit(&mut engine);
        let exploiter = compiled_card("exploiter", "player-0", "Exploit Probe", EXPLOITER);
        engine.state.players[0].battlefield.push(exploiter);
        admit(&mut engine);
        let (player, index) = engine.permanent_position("exploiter").expect("entered");
        let live = engine.state.players[player].battlefield[index].clone();
        engine.enqueue_enter_battlefield_triggers(&live);
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut ExploitFodder)
                .expect("the exploit and its observers resolve");
        }
        assert!(
            engine.permanent_position("fodder").is_none(),
            "the exploit sacrificed the fodder"
        );
        assert_eq!(
            triggers_from(&engine, "skaab"),
            zombies,
            "token fodder {fodder_is_token}"
        );
        let created = engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.definition.is_token && card.definition.type_line.contains("Zombie"))
            .count();
        assert_eq!(created, zombies, "token fodder {fodder_is_token}");
    }
}
