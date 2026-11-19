//! Runtime proofs of the sentence conditional `If CONDITION, EFFECTS.`
//! (CR 608.2c): Oracle text -> canonical `conditional` -> branch execution.
//! The condition is evaluated once, when the instruction resolves, and the
//! referenced object is read with its current characteristics (CR 608.2h).
use super::*;

struct DeclineChoices;

impl DecisionProvider for DeclineChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "these conditional programs ask no question",
        ))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": type_line.starts_with("Creature").then_some("2"),
        "toughness": type_line.starts_with("Creature").then_some("2"),
    }))
    .expect("test definition is valid")
}

fn instance(id: &str, type_line: &str, owner: &str) -> CardInstance {
    let definition = definition(id, type_line);
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        608,
    )
    .expect("test engine is valid")
}

fn rule(name: &str, type_line: &str, mana_cost: &str, text: &str, kind: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let rule = parsed
        .abilities
        .iter()
        .find_map(|ability| ability.rule.as_ref().filter(|rule| rule["kind"] == kind))
        .cloned()
        .unwrap_or_else(|| panic!("{name} emits a {kind}: {:?}", parsed.abilities));
    assert!(rule_is_executable(&rule), "{name}: {rule}");
    rule
}

fn stack(rule: &Value, targets: &[(&str, TargetRef)], decisions: &[(&str, Value)]) -> StackObject {
    StackObject {
        id: "stack:conditional".to_string(),
        controller: "player-0".to_string(),
        card: instance("conditional-source", "Instant", "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(rule["kind"].as_str().unwrap_or("spellAbility").to_string()),
        ability_rule: Some(rule.clone()),
        decisions: decisions
            .iter()
            .map(|(id, value)| (id.to_string(), value.clone()))
            .collect(),
        targets: targets
            .iter()
            .map(|(id, target)| (id.to_string(), target.clone()))
            .collect(),
    }
}

/// Resolves through the stack so target legality at resolution start and
/// object incarnations are established exactly as in a game.
fn resolve(engine: &mut GameEngine, _rule: &Value, stack: &StackObject) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    // Targets are designated (and their incarnations captured) at casting.
    let mut stack = stack.clone();
    for (id, target) in stack.targets.clone() {
        engine
            .capture_target_incarnation(&mut stack.decisions, &id, &target)
            .expect("the fixture target is designated");
    }
    engine.state.stack.push(stack);
    engine
        .resolve_top_stack(&mut DeclineChoices)
        .expect("the conditional program resolves");
}

fn permanent_target(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn controlled_presence_condition_governs_only_its_sentence() {
    // Chilling Trap: the first sentence always applies.
    let rule = rule(
        "Chilling Trap",
        "Instant",
        "{1}{U}",
        "Target creature gets -4/-0 until end of turn. If you control a Wizard, draw a card.",
        "spellAbility",
    );
    for wizard in [false, true] {
        let mut engine = engine();
        engine.state.players[1]
            .battlefield
            .push(instance("victim", "Creature - Bear", "player-1"));
        if wizard {
            engine.state.players[0].battlefield.push(instance(
                "wizard",
                "Creature - Wizard",
                "player-0",
            ));
        }
        let stack = stack(
            &rule,
            &[("targetPermanent", permanent_target("victim"))],
            &[],
        );
        resolve(&mut engine, &rule, &stack);
        let victim = &engine.state.players[1].battlefield[0];
        assert_eq!(engine.current_power(victim), -2);
        assert_eq!(engine.state.players[0].hand.len(), usize::from(wizard));
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_new_boolean_combination_needs_every_operand() {
    // Not a printed card: shared presence and turn atoms under `and`.
    let rule = rule(
        "Combination Probe",
        "Instant",
        "{U}",
        "Draw a card. If you control a Wizard and it's your turn, draw a card.",
        "spellAbility",
    );
    for (wizard, active, drawn) in [(true, 0, 2), (true, 1, 1), (false, 0, 1)] {
        let mut engine = engine();
        engine.state.active_player = active;
        if wizard {
            engine.state.players[0].battlefield.push(instance(
                "wizard",
                "Creature - Wizard",
                "player-0",
            ));
        }
        let stack = stack(&rule, &[], &[]);
        resolve(&mut engine, &rule, &stack);
        assert_eq!(engine.state.players[0].hand.len(), drawn);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_card_target_is_tested_after_the_earlier_instruction_moved_it() {
    // Cemetery Recruitment: the returned card keeps its card characteristics.
    let rule = rule(
        "Cemetery Recruitment",
        "Sorcery",
        "{1}{B}",
        "Return target creature card from your graveyard to your hand. If it's a Zombie card, draw a card.",
        "spellAbility",
    );
    for (type_line, drawn) in [("Creature - Zombie", 1), ("Creature - Bear", 0)] {
        let mut engine = engine();
        engine.state.players[0]
            .graveyard
            .push(instance("returned", type_line, "player-0"));
        let target = TargetRef::Card {
            instance_id: "returned".to_string(),
        };
        let stack = stack(&rule, &[("targetCard", target)], &[]);
        resolve(&mut engine, &rule, &stack);
        let hand = &engine.state.players[0].hand;
        assert!(hand.iter().any(|card| card.instance_id == "returned"));
        assert_eq!(hand.len(), 1 + drawn);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn otherwise_branch_reads_the_same_looked_at_card() {
    // Not a printed card: the `Otherwise` sentence is the else-branch.
    let rule = rule(
        "Otherwise Probe",
        "Sorcery",
        "{2}{G}",
        "Look at the top card of your library. If it's a creature card, put it into your hand. Otherwise, put it into your graveyard.",
        "spellAbility",
    );
    for (type_line, to_hand) in [("Creature - Elf", true), ("Basic Land - Forest", false)] {
        let mut engine = engine();
        engine.state.players[0].library = vec![instance("top", type_line, "player-0")];
        let stack = stack(&rule, &[], &[]);
        resolve(&mut engine, &rule, &stack);
        let player = &engine.state.players[0];
        assert_eq!(
            player.hand.iter().any(|card| card.instance_id == "top"),
            to_hand
        );
        assert_eq!(
            player
                .graveyard
                .iter()
                .any(|card| card.instance_id == "top"),
            !to_hand
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_permanent_target_is_read_with_its_current_state() {
    // Not a printed card: `attacking` exists only on the battlefield object,
    // never on its printed definition.
    let rule = rule(
        "Attack Probe",
        "Instant",
        "{R}",
        "Target creature gets +2/+0 until end of turn. If that creature is attacking, it gains trample until end of turn.",
        "spellAbility",
    );
    for attacking in [true, false] {
        let mut engine = engine();
        let mut attacker = instance("attacker", "Creature - Bear", "player-0");
        attacker.flags.insert("attacking".to_string(), attacking);
        engine.state.players[0].battlefield.push(attacker);
        let stack = stack(
            &rule,
            &[("targetPermanent", permanent_target("attacker"))],
            &[],
        );
        resolve(&mut engine, &rule, &stack);
        let attacker = &engine.state.players[0].battlefield[0];
        assert_eq!(engine.current_power(attacker), 4);
        assert_eq!(engine.permanent_has_keyword(attacker, "trample"), attacking);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn kicker_status_selects_the_branch() {
    // Blink of an Eye: the kicker choice made while casting.
    let rule = rule(
        "Blink of an Eye",
        "Instant",
        "{1}{U}",
        "Kicker {1}{U} (You may pay an additional {1}{U} as you cast this spell.)\nReturn target nonland permanent to its owner's hand. If this spell was kicked, draw a card.",
        "spellAbility",
    );
    for kicked in [false, true] {
        let mut engine = engine();
        engine.state.players[1].battlefield.push(instance(
            "returned",
            "Creature - Bear",
            "player-1",
        ));
        let stack = stack(
            &rule,
            &[("targetPermanent", permanent_target("returned"))],
            &[("kicked", Value::Bool(kicked))],
        );
        resolve(&mut engine, &rule, &stack);
        assert!(engine.state.players[1].battlefield.is_empty());
        assert_eq!(engine.state.players[0].hand.len(), usize::from(kicked));
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_departed_permanent_is_not_read_without_last_known_information() {
    let mut engine = engine();
    engine.state.players[1]
        .graveyard
        .push(instance("departed", "Creature - Human", "player-1"));
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": { "kind": "chosenTarget", "id": "targetPermanent" },
        "where": { "kind": "subtypeContains", "value": "Human" },
    });
    let stack = stack(
        &json!({ "kind": "spellAbility" }),
        &[("targetPermanent", permanent_target("departed"))],
        &[],
    );
    let error = engine
        .checked_object_condition_matches(&condition, &stack, &BTreeMap::new(), &BTreeMap::new())
        .expect_err("a departed permanent needs last-known information");
    assert!(
        error.to_string().contains("last-known information"),
        "{error}"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_collection_condition_rejects_more_than_one_object() {
    let mut engine = engine();
    engine.state.players[0].hand = vec![
        instance("first", "Creature - Elf", "player-0"),
        instance("second", "Creature - Elf", "player-0"),
    ];
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "lookedCards".to_string(),
        RuntimeBinding::Objects(vec!["first".to_string(), "second".to_string()]),
    );
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": { "kind": "boundObjects", "binding": "lookedCards" },
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    let stack = stack(&json!({ "kind": "spellAbility" }), &[], &[]);
    assert!(
        engine
            .checked_object_condition_matches(&condition, &stack, &bindings, &BTreeMap::new())
            .is_err()
    );
    bindings.insert(
        "lookedCards".to_string(),
        RuntimeBinding::Objects(vec!["first".to_string()]),
    );
    assert_eq!(
        engine.checked_object_condition_matches(&condition, &stack, &bindings, &BTreeMap::new()),
        Ok(true)
    );
}
