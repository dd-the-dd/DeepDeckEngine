//! Batch 6 stack and damage effect cells, from real Oracle text to the
//! runtime: redirection shields (CR 614.9) split one damage event and count
//! down, `change the target` admits only complete changes (CR 115.7a) over a
//! single-target declaration, a stack copy applies its copiable-value
//! exceptions (CR 707.10), and `the player or planeswalker it's attacking`
//! reads the declared attack (CR 506.3).
use super::*;

fn parsed_rules(type_line: &str, text: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Generalized Cell".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("stack-damage-cells-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed
        .abilities
        .iter()
        .map(|ability| {
            let rule = ability.rule.clone().expect("primitive IR");
            assert!(rule_is_executable(&rule), "{rule}");
            rule
        })
        .collect()
}

fn parsed_rule(type_line: &str, text: &str) -> Value {
    parsed_rules(type_line, text).remove(0)
}

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.power = Some("2".to_string());
    definition.toughness = Some("5".to_string());
    test_instance(id, definition, controller)
}

fn engine_with_creatures() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("guardian", "player-0")];
    engine.state.players[1].battlefield = vec![creature("ward", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn spell_stack_object(rule: &Value, chosen: Vec<TargetRef>) -> StackObject {
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut targets = BTreeMap::new();
    for (decision, target) in decisions.iter().zip(chosen) {
        targets.insert(decision["id"].as_str().expect("id").to_string(), target);
    }
    StackObject {
        id: "stack:cell".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("cell", test_definition("cell", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, chosen: Vec<TargetRef>) {
    let stack_object = spell_stack_object(rule, chosen);
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the parsed rule resolves");
}

fn player(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn damage(engine: &mut GameEngine, target: TargetRef, amount: i32) -> i32 {
    engine
        .deal_damage_amount_with_kind_and_origin(
            target,
            amount,
            "guardian",
            false,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves")
}

fn life(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .find(|player| player.id == id)
        .expect("player")
        .life
}

fn marked(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("permanent")
        .damage_marked
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn next_n_redirection_splits_one_event_and_counts_down() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "The next 2 damage that would be dealt to target creature this turn is dealt to another target creature instead.",
    );
    assert_eq!(rule["effects"][0]["kind"], "preventDamage");
    assert_eq!(rule["effects"][0]["redirectTo"]["kind"], "chosenTarget");
    resolve(
        &mut engine,
        &rule,
        vec![permanent("ward"), permanent("guardian")],
    );

    // CR 614.9: two of the three damage go to the new recipient, the rest
    // is still dealt to the protected creature.
    assert_eq!(damage(&mut engine, permanent("ward"), 3), 3);
    assert_eq!(marked(&engine, "ward"), 1);
    assert_eq!(marked(&engine, "guardian"), 2);
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "damageRedirected"),
        "the redirection is published"
    );
    // The shield is used up: later damage stays where it is dealt.
    damage(&mut engine, permanent("ward"), 2);
    assert_eq!(marked(&engine, "ward"), 3);
    assert_eq!(marked(&engine, "guardian"), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn all_damage_redirection_moves_every_event_to_the_controller() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "All damage that would be dealt to target creature this turn is dealt to you instead.",
    );
    resolve(&mut engine, &rule, vec![permanent("ward")]);
    let start = life(&engine, "player-0");
    damage(&mut engine, permanent("ward"), 3);
    damage(&mut engine, permanent("ward"), 2);
    assert_eq!(marked(&engine, "ward"), 0);
    assert_eq!(life(&engine, "player-0"), start - 5);
    // Damage to anything else is not covered by the shield.
    damage(&mut engine, permanent("guardian"), 1);
    assert_eq!(marked(&engine, "guardian"), 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn redirection_to_a_departed_recipient_does_nothing_and_is_not_used_up() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "The next 2 damage that would be dealt to target creature this turn is dealt to another target creature instead.",
    );
    resolve(
        &mut engine,
        &rule,
        vec![permanent("ward"), permanent("guardian")],
    );
    let guardian = engine.state.players[0].battlefield.remove(0);
    damage(&mut engine, permanent("ward"), 3);
    assert_eq!(marked(&engine, "ward"), 3);
    engine.state.players[0].battlefield.push(guardian);
    damage(&mut engine, permanent("ward"), 1);
    assert_eq!(
        marked(&engine, "guardian"),
        1,
        "the shield kept its amount while it could not apply"
    );
}

fn targeted_spell(id: &str, targets: Vec<(&str, TargetRef)>) -> StackObject {
    let decisions = targets
        .iter()
        .map(|(decision, _)| {
            json!({
                "id": decision,
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "players" },
            })
        })
        .collect::<Vec<_>>();
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": { "kind": "castingDeclaration", "decisions": decisions },
        "effects": [],
    });
    // A spell on the stack is its card (CR 112.1): no ability kind or rule
    // of its own; its declaration is the card's printed spell ability.
    let mut definition = test_definition(id, "Sorcery");
    definition.rules = vec![rule];
    let mut object = StackObject {
        id: format!("stack:{id}"),
        controller: "player-1".to_string(),
        card: test_instance(id, definition, "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for (decision, target) in targets {
        object.targets.insert(decision.to_string(), target.clone());
        object
            .decisions
            .insert(decision.to_string(), json!([target]));
    }
    object
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn change_the_target_admits_only_a_complete_change_of_a_single_target_spell() {
    let rule = parsed_rule(
        "Instant",
        "Change the target of target spell with a single target.",
    );
    assert_eq!(rule["effects"][0]["kind"], "changeTargets");
    assert_eq!(rule["effects"][0]["mode"], "all");
    let candidates = rule["declaration"]["decisions"][0]["candidates"].clone();
    assert_eq!(candidates["singleTarget"], true);

    let mut engine = test_engine(2);
    engine.state.stack.push(targeted_spell(
        "single",
        vec![("targetPlayer", player("player-0"))],
    ));
    engine.state.stack.push(targeted_spell(
        "double",
        vec![
            ("firstPlayer", player("player-0")),
            ("secondPlayer", player("player-1")),
        ],
    ));
    // The declaration never admits a spell with two targets.
    let legal = engine
        .selection_candidates_in_context("player-0", &candidates, &BTreeMap::new(), true)
        .expect("spell candidates are supported");
    assert_eq!(
        legal,
        vec![TargetRef::StackObject {
            stack_id: "stack:single".to_string()
        }]
    );

    let decision_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("retarget declaration id")
        .to_string();
    let mut deflection = spell_stack_object(&rule, Vec::new());
    deflection.targets.insert(
        decision_id,
        TargetRef::StackObject {
            stack_id: "stack:single".to_string(),
        },
    );
    engine
        .execute_effects(
            &rule["effects"],
            &deflection,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the target changes");
    // CR 115.7a: keeping the old target is not an admissible assignment.
    assert_eq!(
        engine.state.stack[0].targets["targetPlayer"],
        player("player-1")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn stack_copy_applies_its_copiable_value_exceptions() {
    let rule = parsed_rule(
        "Instant",
        "Copy target instant or sorcery spell, except that the copy is red. You may choose new targets for the copy.",
    );
    assert_eq!(rule["effects"][0]["kind"], "copyStackItem");
    assert_eq!(rule["effects"][0]["mayChooseNewTargets"], true);
    assert_eq!(rule["effects"][0]["except"][0]["kind"], "setColors");

    let mut engine = test_engine(2);
    engine
        .state
        .stack
        .push(targeted_spell("original", Vec::new()));
    let decision_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("copy declaration id")
        .to_string();
    let mut fork = spell_stack_object(&rule, Vec::new());
    fork.targets.insert(
        decision_id,
        TargetRef::StackObject {
            stack_id: "stack:original".to_string(),
        },
    );
    engine
        .execute_effects(
            &rule["effects"],
            &fork,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the copy is created");
    assert_eq!(engine.state.stack.len(), 2);
    let copy = &engine.state.stack[1];
    assert!(
        copy.card
            .definition
            .rules
            .iter()
            .any(|rule| { rule["copiable"] == true && rule["colors"] == json!(["R"]) }),
        "the copy is red: {:#?}",
        copy.card.definition.rules
    );
    assert!(
        !engine.state.stack[0]
            .card
            .definition
            .rules
            .iter()
            .any(|rule| rule["copiable"] == true),
        "the original keeps its colors"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn attacked_recipient_reads_the_declared_attack_only_while_attacking() {
    let mut engine = engine_with_creatures();
    let reference = json!({ "kind": "attackedRecipientOf", "object": { "kind": "self" } });
    let source = StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-0".to_string(),
        card: engine.state.players[0].battlefield[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    assert_eq!(
        engine.resolve_target(&reference, &source, &BTreeMap::new()),
        None,
        "a creature that is not attacking names no recipient"
    );
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "guardian".to_string(),
        defender: player("player-1"),
        defending_player_id: "player-1".to_string(),
    });
    assert_eq!(
        engine.resolve_target(&reference, &source, &BTreeMap::new()),
        Some(player("player-1"))
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn nominal_regeneration_amendment_prohibits_the_destroyed_target() {
    let rule = parsed_rule(
        "Sorcery",
        "Destroy target nonartifact creature. That creature can't be regenerated.",
    );
    assert_eq!(rule["effects"][0]["cannotRegenerate"], true);
}
