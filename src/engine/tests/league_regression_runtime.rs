//! Real Oracle text -> primitive IR -> engine behavior for the shared concepts
//! restored for the league regression cohort: plain cycling, mana color
//! distribution, count/presence entering-state exemptions, keyword vocabulary
//! (devoid) and attached-object trigger events.
use super::super::mana_projection_context::ExecutableManaOutputs;
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn parsed_card(
    instance_id: &str,
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle: &str,
) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.mana_cost = mana_cost.to_string();
    definition.rules = parsed_rules(name, type_line, mana_cost, oracle);
    test_instance(instance_id, definition, "player-0")
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_cycling_discards_from_hand_then_draws_on_resolution() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![parsed_card(
        "tower",
        "Raffine's Tower",
        "Land — Plains Island Swamp",
        "",
        "({T}: Add {W}, {U}, or {B}.)\nThis land enters tapped.\nCycling {3} ({3}, Discard this card: Draw a card.)",
    )];
    engine.state.players[0].library = vec![test_instance(
        "next-card",
        test_definition("next-card", "Instant"),
        "player-0",
    )];
    engine.state.players[0].battlefield = (0..3)
        .map(|index| mana_source(&format!("land-{index}"), "Basic Land - Plains", "{W}", None))
        .collect();

    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| action.decisions.get("cycling") == Some(&Value::Bool(true)))
        .expect("cycling is a legal hand activation");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("cycling pays and discards");
    assert!(engine.state.players[0].hand.is_empty());
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "tower")
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|land| land.tapped),
        "the printed {{3}} cost is paid"
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("cycling draws on resolution");
    assert_eq!(
        engine.state.players[0]
            .hand
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        ["next-card"]
    );
}

fn mana_choices(engine: &GameEngine, source: &CardInstance, rules: &[Value]) -> Vec<Vec<String>> {
    let value = &rules[0]["effects"][0]["mana"];
    match engine
        .try_mana_options_for_value(value, "player-0", Some(source))
        .expect("parsed mana value executes")
    {
        ExecutableManaOutputs::Choices(choices) => choices,
        other => panic!("expected executable choices, got {other:?}"),
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_color_distribution_selects_one_color_or_any_combination() {
    let engine = test_engine(2);
    let lotus = parsed_card(
        "lotus",
        "Gilded Lotus",
        "Artifact",
        "{5}",
        "{T}: Add three mana of any one color.",
    );
    let one_color = mana_choices(&engine, &lotus, &lotus.definition.rules);
    assert_eq!(one_color.len(), 5);
    assert!(
        one_color
            .iter()
            .all(|choice| choice.len() == 3 && choice.iter().all(|symbol| *symbol == choice[0]))
    );

    let rules = parsed_rules(
        "Manamorphose",
        "Instant",
        "{1}{R/G}",
        "Add two mana in any combination of colors.\nDraw a card.",
    );
    let combination = mana_choices(&engine, &lotus, &rules);
    assert_eq!(combination.len(), 25);
    assert!(combination.contains(&vec!["W".to_string(), "G".to_string()]));
}

fn play_land(engine: &mut GameEngine, land: CardInstance) -> bool {
    let land_id = land.instance_id.clone();
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![land];
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::PlayLand
                && action.card_instance_id.as_deref() == Some(land_id.as_str())
        })
        .expect("the land play is legal");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("the land is played");
    engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == land_id)
        .expect("the land entered")
        .tapped
}

fn sea_of_clouds() -> CardInstance {
    parsed_card(
        "sea",
        "Sea of Clouds",
        "Land",
        "",
        "This land enters tapped unless you have two or more opponents.\n{T}: Add {W} or {U}.",
    )
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_opponent_count_exemption_reads_the_live_opponent_count() {
    let mut duel = test_engine(2);
    duel.state.players[0].battlefield.clear();
    assert!(
        play_land(&mut duel, sea_of_clouds()),
        "one opponent: tapped"
    );
    let mut multiplayer = test_engine(3);
    multiplayer.state.players[0].battlefield.clear();
    assert!(
        !play_land(&mut multiplayer, sea_of_clouds()),
        "two opponents: untapped"
    );
}

fn canopy_vista() -> CardInstance {
    parsed_card(
        "vista",
        "Canopy Vista",
        "Land — Forest Plains",
        "",
        "({T}: Add {G} or {W}.)\nThis land enters tapped unless you control two or more basic lands.",
    )
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_controlled_count_exemption_without_other_counts_existing_basics() {
    let basics = |count: usize| {
        (0..count)
            .map(|index| {
                mana_source(
                    &format!("basic-{index}"),
                    "Basic Land - Forest",
                    "{G}",
                    None,
                )
            })
            .collect::<Vec<_>>()
    };
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = basics(1);
    assert!(play_land(&mut engine, canopy_vista()), "one basic: tapped");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = basics(2);
    assert!(
        !play_land(&mut engine, canopy_vista()),
        "two basics: untapped"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_named_source_presence_exemption_checks_controlled_legendary_creatures() {
    let minas = || {
        parsed_card(
            "minas",
            "Minas Tirith",
            "Legendary Land",
            "",
            "Minas Tirith enters tapped unless you control a legendary creature.\n{T}: Add {W}.",
        )
    };
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![test_instance(
        "soldier",
        test_definition("soldier", "Creature - Human Soldier"),
        "player-0",
    )];
    assert!(
        play_land(&mut engine, minas()),
        "nonlegendary creature: tapped"
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![test_instance(
        "captain",
        test_definition("captain", "Legendary Creature - Human Soldier"),
        "player-0",
    )];
    assert!(
        !play_land(&mut engine, minas()),
        "legendary creature: untapped"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_devoid_defines_a_colorless_object() {
    let drone = parsed_card(
        "drone",
        "Probe Drone",
        "Creature — Eldrazi Drone",
        "{2}{B}",
        "Devoid (This card has no color.)\nFlying",
    );
    // The color query reads this keyword (see color_set_query_tests).
    assert!(has_keyword(&drone.definition, "devoid"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_equipment_death_trigger_observes_the_equipped_creature() {
    let mut engine = test_engine(2);
    let mut clamp = parsed_card(
        "clamp",
        "Skullclamp",
        "Artifact — Equipment",
        "{1}",
        "Equipped creature gets +1/-1.\nWhenever equipped creature dies, draw two cards.\nEquip {1}",
    );
    clamp.attached_to = Some("bearer".to_string());
    let bearer = test_instance(
        "bearer",
        test_definition("bearer", "Creature - Human"),
        "player-0",
    );
    let bystander = test_instance(
        "bystander",
        test_definition("bystander", "Creature - Human"),
        "player-0",
    );
    engine.state.players[0].battlefield = vec![bearer, bystander, clamp];
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = (0..3)
        .map(|index| {
            test_instance(
                &format!("library-{index}"),
                test_definition(&format!("library-{index}"), "Instant"),
                "player-0",
            )
        })
        .collect();

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .move_permanent_to_graveyard("bystander", "destroyed")
        .expect("an unequipped creature dies");
    assert!(
        engine.state.stack.is_empty(),
        "another creature's death is not observed"
    );
    engine
        .move_permanent_to_graveyard("bearer", "destroyed")
        .expect("the equipped creature dies");
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the equipped creature's death triggers"
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the death trigger resolves");
    assert_eq!(engine.state.players[0].hand.len(), 2);
}

fn spell_stack_object(id: &str, rule: &Value, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: format!("stack:{id}"),
        controller: "player-0".to_string(),
        card: test_instance(id, test_definition(id, "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_sequence_earthbend_declares_its_land_target_and_animates_it() {
    let rules = parsed_rules(
        "Earthbending Lesson",
        "Sorcery — Lesson",
        "{3}{G}",
        "Earthbend 4. (Target land you control becomes a 0/0 creature with haste that's still a land. Put four +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)",
    );
    let rule = &rules[0];
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    let target_id = decision["id"].as_str().expect("a target id").to_string();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![mana_source("forest", "Basic Land - Forest", "{G}", None)];
    let stack = spell_stack_object(
        "lesson",
        rule,
        BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "forest".to_string(),
            },
        )]),
    );
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed Earthbend resolves");
    let forest = &engine.state.players[0].battlefield[0];
    assert_eq!(forest.counters.get("+1/+1"), Some(&4));
    assert!(engine.permanent_is_creature(forest));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_token_characteristic_sentence_creates_tokens_with_the_ability() {
    let rules = parsed_rules(
        "Brood Monitor",
        "Creature — Eldrazi Drone",
        "{4}{G}{G}",
        "Devoid (This card has no color.)\nWhen this creature enters, create three 1/1 colorless Eldrazi Scion creature tokens. They have \"Sacrifice this token: Add {C}.\"",
    );
    let trigger = rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .expect("the entry trigger");
    let create = &trigger["effects"][0];
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    let stack = spell_stack_object("monitor", trigger, BTreeMap::new());
    engine
        .create_tokens(create, &stack, &BTreeMap::new(), &BTreeMap::new())
        .expect("the parsed Scion tokens are created");
    let tokens = &engine.state.players[0].battlefield;
    assert_eq!(tokens.len(), 3);
    assert!(tokens.iter().all(|token| {
        token
            .definition
            .rules
            .iter()
            .any(|rule| rule["kind"] == "manaAbility")
    }));
}
