//! Real Oracle text -> IR -> engine behavior for batch 11's stack and mana
//! riders: a variable copy count read at resolution (CR 707.10), Miracle
//! (CR 702.94a), a gift given before the spell's other effects (CR 702.174a)
//! and mana retained through step ends (CR 106.4, 500.4).
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
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

fn spell(instance_id: &str, name: &str, type_line: &str, cost: &str, oracle: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.mana_cost = cost.to_string();
    definition.rules = parsed_rules(name, type_line, cost, oracle);
    test_instance(instance_id, definition, "player-0")
}

fn pool(symbols: &[&str]) -> Vec<FloatingMana> {
    symbols
        .iter()
        .map(|symbol| FloatingMana {
            symbol: (*symbol).to_string(),
            spend_restriction: None,
        })
        .collect()
}

fn main_phase_engine(hand: Vec<CardInstance>, mana: &[&str]) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].hand = hand;
    engine.state.players[0].mana_pool = pool(mana);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn resolve_stack(engine: &mut GameEngine) {
    let mut guard = 0;
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut CastSpellDecisionProvider)
            .expect("the stack resolves");
        guard += 1;
        assert!(guard < 32, "the stack terminates");
    }
}

/// CR 903.8, 707.10: `copy it for each time you've cast your commander from
/// the command zone this game` counts the record as the trigger resolves.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_commander_cast_count_copies_the_spell_that_many_times() {
    let storm = spell(
        "storm",
        "Tidal Storm",
        "Sorcery",
        "{1}{G}",
        "When you cast this spell, copy it for each time you've cast your commander from the command zone this game.\nYou gain 2 life.",
    );
    let trigger = storm.definition.rules[0].clone();
    assert_eq!(trigger["effects"][0]["kind"], "copyStackItem");
    assert_eq!(
        trigger["effects"][0]["times"]["kind"],
        "commanderCastsFromCommandZone"
    );
    let mut engine = main_phase_engine(vec![storm], &["G", "C"]);
    engine.state.commanders.push(CommanderState {
        card_instance_id: "commander".to_string(),
        name: "Commander".to_string(),
        owner_id: "player-0".to_string(),
        casts_from_command_zone: 2,
    });
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("storm")
        })
        .expect("the spell can be cast");
    let life_before = engine.state.players[0].life;
    engine
        .apply_priority_action(&cast, &mut CastSpellDecisionProvider)
        .expect("cast");
    resolve_stack(&mut engine);
    assert_eq!(
        engine.state.players[0].life - life_before,
        6,
        "two copies and the original each gain 2 life"
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "stackObjectCopied")
            .count(),
        2
    );
}

/// CR 702.94a: the first card drawn this turn is revealed and may be cast
/// for its miracle cost, read from the printed keyword line.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_parsed_miracle_casts_the_first_card_drawn_for_its_cost() {
    let mastery = spell(
        "mastery",
        "Temporal Mastery",
        "Sorcery",
        "{5}{U}{U}",
        "Take an extra turn after this one. Exile Temporal Mastery.\nMiracle {1}{U} (You may cast this card for its miracle cost when you draw it if it's the first card you drew this turn.)",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::Draw;
    engine.state.active_player = 0;
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![mastery];
    engine.state.players[0].mana_pool = pool(&["U", "C"]);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.draw_cards("player-0", 1).expect("card is drawn");
    assert!(
        engine.state.stack.iter().any(|object| {
            object
                .ability_rule
                .as_ref()
                .is_some_and(|rule| rule["effects"][0]["operation"] == "miracleCastDrawnCard")
        }),
        "the miracle trigger is put on the stack"
    );
    engine
        .resolve_top_stack(&mut CastSpellDecisionProvider)
        .expect("miracle trigger resolves");
    assert!(
        engine.state.players[0].mana_pool.is_empty(),
        "{{1}}{{U}} paid"
    );
    assert!(
        engine
            .state
            .stack
            .iter()
            .any(|object| object.card.instance_id == "mastery" && object.ability_rule.is_none()),
        "the drawn card is cast"
    );
}

/// CR 702.174a: the promised gift is given before the spell's other
/// effects; its recipient is the opponent chosen as the spell was cast.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_promised_extra_turn_gift_precedes_the_other_effects() {
    let perch = spell(
        "perch",
        "Perch Gift",
        "Instant",
        "{W}",
        "Gift an extra turn (You may promise an opponent a gift as you cast this spell. If you do, they take an extra turn after this one.)\nYou gain 3 life.",
    );
    let mut engine = main_phase_engine(vec![perch], &["W"]);
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("perch")
                && action.decisions.get("giftRecipient") == Some(&json!("player-1"))
        })
        .expect("the gift can be promised to the opponent");
    engine
        .apply_priority_action(&cast, &mut CastSpellDecisionProvider)
        .expect("cast with the gift promised");
    resolve_stack(&mut engine);
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        modifier["kind"] == "extraTurnForPlayer" && modifier["playerId"] == "player-1"
    }));
    let position = |kind: &str| {
        engine
            .state
            .events
            .iter()
            .position(|event| event.kind == kind)
            .unwrap_or_else(|| panic!("{kind} recorded"))
    };
    assert!(position("giftPromised") < position("lifeGained"));
}

/// CR 702.174a: a token gift is created under the recipient's control.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_promised_treasure_gift_is_created_for_the_recipient() {
    let blast = spell(
        "blast",
        "Treasure Gift",
        "Instant",
        "{R}",
        "Gift a Treasure (You may promise an opponent a gift as you cast this spell. If you do, they create a Treasure token before its other effects. It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")\nYou gain 1 life.",
    );
    let mut engine = main_phase_engine(vec![blast], &["R"]);
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("blast")
                && action.decisions.get("giftRecipient") == Some(&json!("player-1"))
        })
        .expect("the gift can be promised");
    engine
        .apply_priority_action(&cast, &mut CastSpellDecisionProvider)
        .expect("cast with the gift promised");
    resolve_stack(&mut engine);
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|permanent| permanent.definition.name == "Treasure"),
        "the recipient creates the Treasure"
    );
    assert!(
        !engine.state.players[0]
            .battlefield
            .iter()
            .any(|permanent| permanent.definition.name == "Treasure")
    );
}

/// CR 106.4, 500.4: retained mana survives every step end of the turn and
/// is lost as the cleanup step ends; other mana empties at once. The
/// retention is a unit attribute: the unit still pays for anything.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn retained_mana_survives_steps_until_the_turn_ends() {
    let rules = parsed_rules(
        "Sakura-Tribe Springcaller",
        "Creature — Snake Shaman",
        "{3}{G}",
        "At the beginning of your upkeep, add {G}. Until end of turn, you don't lose this mana as steps and phases end.",
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.step = GameStep::Upkeep;
    let mut definition = test_definition("springcaller", "Creature — Snake Shaman");
    definition.rules = rules.clone();
    let source = test_instance("springcaller", definition, "player-0");
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[0].mana_pool = pool(&["R"]);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let _ = engine.put_triggered_ability_on_stack_with_decisions(
        &source,
        rules[0].clone(),
        BTreeMap::new(),
    );
    resolve_stack(&mut engine);
    let symbols = |engine: &GameEngine| {
        engine.state.players[0]
            .mana_pool
            .iter()
            .map(|mana| mana.symbol.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(symbols(&engine), ["R", "G"]);
    let retained = engine.state.players[0].mana_pool[1].clone();
    assert!(
        super::super::mana_postcost_ordered::mana_pool_entry_can_pay(&retained, None, None, None)
    );
    engine.transition_to_step(GameStep::Draw);
    assert_eq!(symbols(&engine), ["G"], "only the retained unit survives");
    engine.transition_to_step(GameStep::PrecombatMain);
    engine.transition_to_step(GameStep::EndStep);
    engine.transition_to_step(GameStep::Cleanup);
    assert_eq!(symbols(&engine), ["G"], "kept into the cleanup step");
    engine.transition_to_step(GameStep::Untap);
    assert!(symbols(&engine).is_empty(), "lost as the turn ends");
}
