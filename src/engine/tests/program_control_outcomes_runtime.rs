//! Runtime proofs of program control (CR 608.2c): a replacing `instead`
//! sentence resolves exactly one of its two branches, its condition is read
//! once where the replaced sentence begins, and a target both branches act on
//! is one declaration under a cast choice. Oracle text -> canonical rule ->
//! resolution through the stack.
use super::*;

struct DeclineChoices;

impl DecisionProvider for DeclineChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new("these replacing programs ask no question"))
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
        6082,
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

fn resolve(
    engine: &mut GameEngine,
    rule: &Value,
    targets: &[(&str, TargetRef)],
    decisions: &[(&str, Value)],
) {
    let mut stack = StackObject {
        id: "stack:replacing".to_string(),
        controller: "player-0".to_string(),
        card: instance("replacing-source", "Instant", "player-0"),
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
    };
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for (id, target) in stack.targets.clone() {
        engine
            .capture_target_incarnation(&mut stack.decisions, &id, &target)
            .expect("the fixture target is designated");
    }
    engine.state.stack.push(stack);
    engine
        .resolve_top_stack(&mut DeclineChoices)
        .expect("the replacing program resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_replacing_sentence_resolves_exactly_one_branch_in_either_position() {
    for text in [
        "You gain 2 life. If you control a Wizard, you gain 5 life instead.",
        "You gain 2 life. If you control a Wizard, instead you gain 5 life.",
    ] {
        let rule = rule("Synthetic Blessing", "Instant", "{W}", text, "spellAbility");
        for wizard in [false, true] {
            let mut engine = engine();
            if wizard {
                engine.state.players[0].battlefield.push(instance(
                    "wizard",
                    "Creature - Wizard",
                    "player-0",
                ));
            }
            resolve(&mut engine, &rule, &[], &[]);
            // The replacing branch is never added to the replaced one.
            let expected = if wizard { 25 } else { 22 };
            assert_eq!(
                engine.state.players[0].life, expected,
                "{text} wizard={wizard}"
            );
        }
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_kicked_replacement_acts_on_the_one_declared_target() {
    let rule = rule(
        "Synthetic Weakening",
        "Instant",
        "{B}",
        "Kicker {1}\nTarget creature gets -1/-1 until end of turn. If this spell was kicked, that creature gets -2/-0 until end of turn instead.",
        "spellAbility",
    );
    let targets = rule["declaration"]["decisions"]
        .as_array()
        .expect("the spell declares its target")
        .iter()
        .filter(|decision| decision["kind"] == "chooseTargets")
        .collect::<Vec<_>>();
    // Both branches act on the one declared creature.
    assert_eq!(targets.len(), 1, "{rule}");
    let target_id = targets[0]["id"]
        .as_str()
        .expect("one target declaration")
        .to_string();
    for kicked in [false, true] {
        let mut engine = engine();
        engine.state.players[1]
            .battlefield
            .push(instance("victim", "Creature - Bear", "player-1"));
        resolve(
            &mut engine,
            &rule,
            &[(
                target_id.as_str(),
                TargetRef::Permanent {
                    instance_id: "victim".to_string(),
                },
            )],
            &[("kicked", Value::Bool(kicked))],
        );
        let victim = &engine.state.players[1].battlefield[0];
        let expected = if kicked { (0, 2) } else { (1, 1) };
        assert_eq!(
            (
                engine.current_power(victim),
                engine.current_toughness(victim)
            ),
            expected,
            "kicked={kicked}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_coordinated_permission_is_one_optional_choice_over_one_target() {
    let rule = rule(
        "Synthetic Angler",
        "Creature — Merfolk Wizard",
        "{1}{U}",
        "{1}{U}, {T}: You may tap or untap target creature.",
        "activatedAbility",
    );
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .expect("the activation declares its target");
    assert_eq!(decisions.len(), 1, "{rule}");
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "optionalEffects", "{rule}");
    let options = effect["effects"][0]["options"]
        .as_array()
        .expect("one choice between the coordinated actions");
    assert_eq!(options.len(), 2, "{rule}");
    assert_eq!(options[0]["effects"][0]["kind"], "tapPermanent", "{rule}");
    assert!(
        matches!(
            options[1]["effects"][0]["kind"].as_str(),
            Some("untapPermanent" | "untapPermanents")
        ),
        "{rule}"
    );
}
