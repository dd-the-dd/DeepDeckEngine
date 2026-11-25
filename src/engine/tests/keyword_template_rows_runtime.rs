//! Keyword rows of batch 6 compiled from real Oracle text and run on the
//! engine: ascend's city's blessing (CR 702.131), ninjutsu's attacking
//! entry (CR 702.49) and myriad's attack trigger (CR 702.116).
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("every ability has a rule"))
        .flat_map(|rule| crate::oracle::keyword_expansion_rules(&rule))
        .inspect(|rule| assert!(rule_is_executable(rule), "{name}: {rule:#}"))
        .collect()
}

fn card(id: &str, type_line: &str, mana_cost: &str, rules: Vec<Value>) -> CardDefinition {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.rules = rules;
    definition
}

/// Takes the first offered option: `create`, the first defender, the first
/// payment.
struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn blessed(engine: &GameEngine, player_id: &str) -> bool {
    engine.condition_value(
        &json!({
            "kind": "hasCityBlessing",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        }),
        player_id,
        &BTreeMap::new(),
    )
}

const DUSK_CHARGER: &str = "Ascend (If you control ten or more permanents, you get the city's blessing for the rest of the game.)\nThis creature gets +2/+2 as long as you have the city's blessing.";

fn ascend_engine(lands: usize) -> GameEngine {
    let rules = parsed_rules(
        "Dusk Charger",
        "Creature \u{2014} Horse",
        "{3}{B}",
        DUSK_CHARGER,
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.players[0].battlefield = vec![test_instance(
        "dusk-charger",
        card("dusk-charger", "Creature - Horse", "{3}{B}", rules),
        "player-0",
    )];
    for index in 0..lands {
        engine.state.players[0].battlefield.push(mana_source(
            &format!("swamp-{index}"),
            "Basic Land - Swamp",
            "{B}",
            None,
        ));
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

#[test]
fn ascend_grants_the_city_blessing_for_the_rest_of_the_game() {
    // Nine permanents: no blessing (CR 702.131b).
    let mut short = ascend_engine(8);
    short.check_state_based_actions();
    assert!(!blessed(&short, "player-0"));

    let mut engine = ascend_engine(9);
    engine.check_state_based_actions();
    assert!(blessed(&engine, "player-0"));
    assert!(!blessed(&engine, "player-1"));
    // The designation stays when the permanents are gone (CR 702.131c).
    engine.state.players[0].battlefield.truncate(1);
    engine.check_state_based_actions();
    assert!(blessed(&engine, "player-0"));
    assert_eq!(
        engine
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| value_kind(modifier) == Some("cityBlessing"))
            .count(),
        1,
        "one designation, never granted twice"
    );
}

#[test]
fn ascend_on_a_sorcery_checks_its_controller_as_the_spell_resolves() {
    let ritual = parsed_rules(
        "Generic Golden Ritual",
        "Sorcery",
        "{B}",
        "Ascend (If you control ten or more permanents, you get the city's blessing for the rest of the game.)\nYou gain 2 life.",
    );
    for (lands, expected) in [(9, false), (10, true)] {
        let mut engine = test_engine(2);
        engine.state.active_player = 0;
        engine.state.priority_player = Some(0);
        engine.state.step = GameStep::PrecombatMain;
        engine.state.players[0].hand = vec![test_instance(
            "ritual",
            card("ritual", "Sorcery", "{B}", ritual.clone()),
            "player-0",
        )];
        engine.state.players[0].battlefield = (0..lands)
            .map(|index| mana_source(&format!("swamp-{index}"), "Basic Land - Swamp", "{B}", None))
            .collect();
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let cast = engine
            .legal_priority_actions(0)
            .into_iter()
            .find(|action| {
                action.kind == ActionKind::CastSpell
                    && action.card_instance_id.as_deref() == Some("ritual")
            })
            .expect("the sorcery can be cast");
        engine
            .apply_priority_action(&cast, &mut FirstOption)
            .expect("the sorcery is cast");
        engine
            .resolve_top_stack(&mut FirstOption)
            .expect("the sorcery resolves");
        assert_eq!(blessed(&engine, "player-0"), expected, "{lands} lands");
        assert_eq!(engine.state.players[0].life, 22);
    }
}

const NINJA_OF_THE_DEEP_HOURS: &str = "Ninjutsu {1}{U} ({1}{U}, Return an unblocked attacker you control to hand: Put this card onto the battlefield from your hand tapped and attacking.)\nWhenever this creature deals combat damage to a player, you may draw a card.";

#[test]
fn ninjutsu_returns_an_unblocked_attacker_and_attacks_its_defender() {
    let ninja = parsed_rules(
        "Ninja of the Deep Hours",
        "Creature \u{2014} Human Ninja",
        "{3}{U}",
        NINJA_OF_THE_DEEP_HOURS,
    );
    let mut engine = test_engine(3);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::DeclareBlockers;
    let mut bear = test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    );
    bear.flags.insert("attacking".to_string(), true);
    engine.state.players[0].battlefield = vec![
        bear,
        mana_source("island-0", "Basic Land - Island", "{U}", None),
        mana_source("island-1", "Basic Land - Island", "{U}", None),
    ];
    engine.state.players[0].hand = vec![test_instance(
        "ninja",
        card("ninja", "Creature - Human Ninja", "{3}{U}", ninja),
        "player-0",
    )];
    // The bear attacks the second opponent, not the first one.
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "bear".to_string(),
        defender: TargetRef::Player {
            player_id: "player-2".to_string(),
        },
        defending_player_id: "player-2".to_string(),
    }];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let activation = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("ninja")
        })
        .expect("ninjutsu is offered from hand for the unblocked attacker");
    engine
        .apply_priority_action(&activation, &mut FirstOption)
        .expect("ninjutsu is activated");
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "bear"),
        "the attacker returned to its owner's hand as a cost"
    );
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("ninjutsu resolves");
    let (player, index) = engine
        .permanent_position("ninja")
        .expect("the ninja entered the battlefield");
    let entered = &engine.state.players[player].battlefield[index];
    assert!(entered.tapped);
    assert_eq!(entered.flags.get("attacking"), Some(&true));
    let attack = engine
        .state
        .combat
        .attackers
        .iter()
        .find(|attacker| attacker.attacker_id == "ninja")
        .expect("the ninja is attacking");
    assert_eq!(attack.defending_player_id, "player-2");
    assert_eq!(
        attack.defender,
        TargetRef::Player {
            player_id: "player-2".to_string(),
        }
    );
}

#[test]
fn ninjutsu_is_not_offered_without_an_unblocked_attacker() {
    let ninja = parsed_rules(
        "Ninja of the Deep Hours",
        "Creature \u{2014} Human Ninja",
        "{3}{U}",
        NINJA_OF_THE_DEEP_HOURS,
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        test_instance(
            "bear",
            test_definition("bear", "Creature - Bear"),
            "player-0",
        ),
        mana_source("island-0", "Basic Land - Island", "{U}", None),
        mana_source("island-1", "Basic Land - Island", "{U}", None),
    ];
    engine.state.players[0].hand = vec![test_instance(
        "ninja",
        card("ninja", "Creature - Human Ninja", "{3}{U}", ninja),
        "player-0",
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(!engine.legal_priority_actions(0).into_iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some("ninja")
    }));
}

/// CR 509.1h: an attacker is not unblocked before blockers are declared, so
/// ninjutsu is not offered in the declare-attackers step even though an
/// attacker has been declared and nothing blocks it yet.
#[test]
fn ninjutsu_is_not_offered_before_blockers_are_declared() {
    let ninja = parsed_rules(
        "Ninja of the Deep Hours",
        "Creature \u{2014} Human Ninja",
        "{3}{U}",
        NINJA_OF_THE_DEEP_HOURS,
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::DeclareAttackers;
    let mut bear = test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    );
    bear.flags.insert("attacking".to_string(), true);
    engine.state.players[0].battlefield = vec![
        bear,
        mana_source("island-0", "Basic Land - Island", "{U}", None),
        mana_source("island-1", "Basic Land - Island", "{U}", None),
    ];
    engine.state.players[0].hand = vec![test_instance(
        "ninja",
        card("ninja", "Creature - Human Ninja", "{3}{U}", ninja),
        "player-0",
    )];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "bear".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let offered = |engine: &GameEngine| {
        engine.legal_priority_actions(0).into_iter().any(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("ninja")
        })
    };
    assert!(!offered(&engine), "no unblocked attacker before blocks");
    // The same board offers it once the declare-blockers step has begun.
    engine.state.step = GameStep::DeclareBlockers;
    assert!(offered(&engine));
}

const CALLER_OF_THE_PACK: &str = "Trample\nMyriad (Whenever this creature attacks, for each opponent other than defending player, you may create a token copy that's tapped and attacking that player or a planeswalker they control. Exile the tokens at end of combat.)";

fn myriad_engine(players: usize) -> GameEngine {
    let caller = parsed_rules(
        "Caller of the Pack",
        "Creature \u{2014} Beast",
        "{5}{G}{G}",
        CALLER_OF_THE_PACK,
    );
    let mut engine = test_engine(players);
    engine.state.active_player = 0;
    engine.state.step = GameStep::DeclareAttackers;
    let mut attacker = test_instance(
        "caller",
        card("caller", "Creature - Beast", "{5}{G}{G}", caller),
        "player-0",
    );
    attacker.flags.insert("attacking".to_string(), true);
    engine.state.players[0].battlefield = vec![attacker];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "caller".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn myriad_triggers(engine: &GameEngine) -> usize {
    engine
        .state
        .stack
        .iter()
        .filter(|object| {
            object
                .ability_rule
                .as_ref()
                .is_some_and(|rule| rule["effects"][0]["kind"] == "myriadAttackingCopies")
        })
        .count()
}

#[test]
fn myriad_copies_attack_each_other_opponent_and_are_exiled_at_end_of_combat() {
    let mut engine = myriad_engine(3);
    engine.enqueue_declared_attacker_triggers(&["caller".to_string()]);
    assert_eq!(myriad_triggers(&engine), 1);
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut FirstOption)
            .expect("the myriad trigger resolves");
    }
    let copies = engine
        .state
        .combat
        .attackers
        .iter()
        .filter(|attacker| attacker.attacker_id != "caller")
        .cloned()
        .collect::<Vec<_>>();
    let [copy] = copies.as_slice() else {
        panic!("one token copy for the one other opponent: {copies:#?}");
    };
    assert_eq!(copy.defending_player_id, "player-2");
    let (player, index) = engine
        .permanent_position(&copy.attacker_id)
        .expect("the token copy is on the battlefield");
    let token = &engine.state.players[player].battlefield[index];
    assert!(token.tapped);
    assert_eq!(token.definition.name, "caller");
    // Tokens put onto the battlefield attacking never trigger myriad.
    assert_eq!(myriad_triggers(&engine), 0);

    assert_eq!(engine.fire_delayed_step_triggers("endOfCombat"), 1);
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut FirstOption)
            .expect("the exile trigger resolves");
    }
    assert!(engine.permanent_position(&copy.attacker_id).is_none());
    assert!(engine.permanent_position("caller").is_some());
}

#[test]
fn myriad_in_a_two_player_game_creates_nothing() {
    let mut engine = myriad_engine(2);
    engine.enqueue_declared_attacker_triggers(&["caller".to_string()]);
    assert_eq!(myriad_triggers(&engine), 1);
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("the myriad trigger resolves");
    assert_eq!(engine.state.combat.attackers.len(), 1);
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
}

#[test]
fn bestow_casts_the_creature_as_an_aura_attached_to_a_creature() {
    let spirit = parsed_rules(
        "Generic Bestowed Spirit",
        "Enchantment Creature \u{2014} Spirit",
        "{1}{W}",
        "Bestow {2}{W} (If you cast this card for its bestow cost, it's an Aura spell with enchant creature. It becomes a creature again if it's not attached.)\nFlying",
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![test_instance(
        "spirit",
        card("spirit", "Enchantment Creature - Spirit", "{1}{W}", spirit),
        "player-0",
    )];
    let mut battlefield = vec![test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    )];
    for index in 0..3 {
        battlefield.push(mana_source(
            &format!("plains-{index}"),
            "Basic Land - Plains",
            "{W}",
            None,
        ));
    }
    engine.state.players[0].battlefield = battlefield;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("spirit")
                && action.decisions.get("useBestow") == Some(&Value::Bool(true))
        })
        .expect("the bestow cast is offered with a creature to enchant");
    engine
        .apply_priority_action(&cast, &mut FirstOption)
        .expect("the bestow cost is paid");
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("the bestowed Aura resolves");
    let (player, index) = engine
        .permanent_position("spirit")
        .expect("the Aura is on the battlefield");
    let aura = &engine.state.players[player].battlefield[index];
    assert_eq!(aura.attached_to.as_deref(), Some("bear"));
    assert!(aura.definition.type_line.contains("Aura"));
}
