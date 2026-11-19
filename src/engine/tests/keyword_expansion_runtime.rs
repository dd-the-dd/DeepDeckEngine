//! Compiled Oracle keyword lines and keyword actions executed through the
//! existing runtime primitives: Split second (CR 702.61), Dredge (CR 702.52)
//! and doubling counters (CR 701.10e).
use super::*;

fn compiled_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{G}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical ability has a rule"))
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule}"))
        .collect()
}

fn creature_with_counters(id: &str, counters: i32, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Hydra");
    definition.power = Some("0".to_string());
    definition.toughness = Some("0".to_string());
    let mut instance = test_instance(id, definition, owner);
    if counters > 0 {
        instance.counters.insert("+1/+1".to_string(), counters);
    }
    instance
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn compiled_split_second_leaves_only_mana_abilities_and_passing() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    let mut instant = test_definition("held-instant", "Instant");
    instant.mana_cost = "{0}".to_string();
    instant.rules = compiled_rules("Probe Instant", "Instant", "Draw a card.");
    engine.state.players[0].hand = vec![test_instance("held-instant", instant, "player-0")];

    let mut split = test_definition("wipe-away", "Instant");
    split.rules = compiled_rules(
        "Wipe Away",
        "Instant",
        "Split second (As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities.)\nDraw a card.",
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(
        engine
            .legal_priority_actions(0)
            .iter()
            .any(|action| action.kind == ActionKind::CastSpell),
        "the instant is castable before the split-second spell is on the stack"
    );
    engine.state.stack.push(StackObject {
        id: "split-stack".to_string(),
        controller: "player-1".to_string(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        card: test_instance("wipe-away", split, "player-1"),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    assert!(
        engine
            .legal_priority_actions(0)
            .iter()
            .all(|action| action.kind != ActionKind::CastSpell),
        "no spell can be cast while the compiled split-second spell is on the stack"
    );
}

struct AcceptDredge;

impl DecisionProvider for AcceptDredge {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        request
            .options
            .iter()
            .position(|option| option.label.to_ascii_lowercase().contains("dredge"))
            .ok_or_else(|| EngineError::new("expected a legal dredge replacement"))
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn compiled_dredge_replaces_a_draw_with_its_printed_count() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].graveyard.clear();
    engine.state.players[0].library = (0..4)
        .map(|index| {
            test_instance(
                &format!("library-{index}"),
                test_definition(&format!("library-{index}"), "Basic Land - Forest"),
                "player-0",
            )
        })
        .collect();
    let mut loam = test_definition("life-from-the-loam", "Sorcery");
    loam.rules = compiled_rules(
        "Life from the Loam",
        "Sorcery",
        "Dredge 3 (If you would draw a card, you may mill three cards instead. If you do, return this card from your graveyard to your hand.)",
    );
    engine.state.players[0]
        .graveyard
        .push(test_instance("life-from-the-loam", loam, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .draw_cards_with_replacements("player-0", 1, &mut AcceptDredge)
        .expect("the compiled dredge replaces the draw");

    assert_eq!(
        engine.state.players[0].library.len(),
        1,
        "three cards milled"
    );
    assert_eq!(engine.state.players[0].graveyard.len(), 3);
    assert_eq!(
        engine.state.players[0].hand[0].instance_id,
        "life-from-the-loam"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn compiled_double_counters_gives_the_target_as_many_as_it_has() {
    // Growth Curve: the ordinary counter action, then the doubling expansion
    // reads the same target's count after the first counter was placed.
    let mut spell = test_definition("growth-curve", "Sorcery");
    spell.mana_cost = "{G}".to_string();
    spell.rules = compiled_rules(
        "Growth Curve",
        "Sorcery",
        "Put a +1/+1 counter on target creature you control, then double the number of +1/+1 counters on that creature.",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "G".to_string(),
        spend_restriction: None,
    }];
    engine.state.players[0].hand = vec![test_instance("growth-curve", spell, "player-0")];
    engine.state.players[0].battlefield = vec![
        creature_with_counters("hydra", 2, "player-0"),
        creature_with_counters("bystander", 4, "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.targets.values().any(|target| {
                    target
                        == &TargetRef::Permanent {
                            instance_id: "hydra".to_string(),
                        }
                })
        })
        .expect("Growth Curve targets the hydra");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(7))
        .expect("Growth Curve is cast");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(8))
        .expect("Growth Curve resolves");
    let counters = |id: &str| {
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|permanent| permanent.instance_id == id)
            .and_then(|permanent| permanent.counters.get("+1/+1").copied())
    };
    assert_eq!(counters("hydra"), Some(6), "(2 + 1) doubled");
    assert_eq!(
        counters("bystander"),
        Some(4),
        "only the bound target doubles"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn compiled_double_counters_on_the_source_reads_its_own_count() {
    // An unused combination: a self-referential doubling in an activated
    // ability (no printed card required) composes without production code.
    let rules = compiled_rules(
        "Doubling Probe",
        "Creature - Hydra",
        "{T}: Double the number of +1/+1 counters on this creature.",
    );
    let effects = &rules[0]["effects"];
    assert_eq!(
        effects[0]["count"],
        json!({ "kind": "counterCountOf", "object": { "kind": "self" }, "counter": "+1/+1" })
    );
}

/// Picks the largest offered selection, so every eligible object is chosen.
struct ChooseEverything;

impl DecisionProvider for ChooseEverything {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .enumerate()
            .max_by_key(|(_, option)| {
                option
                    .decisions
                    .values()
                    .filter_map(Value::as_array)
                    .map(Vec::len)
                    .sum::<usize>()
            })
            .map(|(index, _)| index)
            .unwrap_or(0))
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn compiled_proliferate_adds_one_counter_of_each_kind_to_chosen_objects() {
    let mut spell = test_definition("proliferate-spell", "Sorcery");
    spell.rules = compiled_rules(
        "Proliferate Probe",
        "Sorcery",
        "Proliferate. (Choose any number of permanents and/or players, then give each another counter of each kind already there.)",
    );
    assert_eq!(spell.rules[0]["effects"][0]["kind"], "proliferate");
    let mut engine = test_engine(2);
    let mut charged = creature_with_counters("charged", 2, "player-0");
    charged.counters.insert("charge".to_string(), 1);
    engine.state.players[0].battlefield =
        vec![charged, creature_with_counters("plain", 0, "player-0")];
    engine.state.players[1]
        .counters
        .insert("poison".to_string(), 3);
    let stack_object = StackObject {
        id: "stack:proliferate".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("proliferate-spell", spell.clone(), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let mut decisions = BTreeMap::new();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .execute_effects(
            &spell.rules[0]["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut ChooseEverything,
        )
        .expect("proliferate resolves");
    let charged = &engine.state.players[0].battlefield[0];
    assert_eq!(charged.counters.get("+1/+1"), Some(&3));
    assert_eq!(charged.counters.get("charge"), Some(&2));
    assert!(engine.state.players[0].battlefield[1].counters.is_empty());
    assert_eq!(engine.state.players[1].counters.get("poison"), Some(&4));
}
