//! Real Oracle text -> primitive IR -> engine behavior for the antecedent
//! producers of batch 11: the genitive of the source as a performer
//! (`this creature's owner`, CR 108.3) whose predicate names the source, the
//! last known controller of a departed enchanted permanent (CR 603.10a,
//! 608.2h), and the player a batched combat-damage event records (CR 510.2,
//! 603.2c).
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Producer Probe".to_string(),
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn permanent(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    test_instance(id, definition, owner)
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
    }
}

/// The rule of `rules` whose kind is `kind`.
fn rule_of_kind(rules: &[Value], kind: &str) -> Value {
    rules
        .iter()
        .find(|rule| rule["kind"].as_str() == Some(kind))
        .unwrap_or_else(|| panic!("a {kind} rule in {rules:#?}"))
        .clone()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn possessive_source_owner_shuffles_the_source_into_its_owners_library() {
    // Cerulean Sphinx: the sphinx is owned by player-1 and controlled by
    // player-0. Its owner (CR 108.3), not its controller, shuffles it into
    // the owner's library; `it` names the source the genitive named.
    let rules = parsed_rules(
        "Creature — Sphinx",
        "Flying\n{U}: This creature's owner shuffles it into their library.",
    );
    let rule = rule_of_kind(&rules, "activatedAbility");
    let mut engine = test_engine(2);
    let mut sphinx = permanent("sphinx", "Creature — Sphinx", rules.clone(), "player-1");
    sphinx.controller = "player-0".to_string();
    engine.state.players[0].battlefield.push(sphinx.clone());
    admit(&mut engine);
    let stack = StackObject {
        id: "stack:sphinx".to_string(),
        controller: "player-0".to_string(),
        card: sphinx,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: rule["kind"].as_str().map(str::to_string),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the ability resolves");
    assert!(
        engine.permanent_position("sphinx").is_none(),
        "the source left the battlefield"
    );
    assert!(
        engine.state.players[1]
            .library
            .iter()
            .any(|card| card.instance_id == "sphinx"),
        "the owner's library holds the source"
    );
    assert!(
        !engine.state.players[0]
            .library
            .iter()
            .any(|card| card.instance_id == "sphinx"),
        "never the controller's library"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn departed_enchanted_creature_controller_is_its_last_known_controller() {
    // A Funeral March-like Aura of player-0 enchants a creature owned by
    // player-1 and controlled by player-2. When the creature leaves the
    // battlefield, its controller as it last existed loses the life (CR
    // 603.10a, 608.2h), not its owner and not the Aura's controller.
    let rules = parsed_rules(
        "Enchantment — Aura",
        "Enchant creature\nWhen enchanted creature leaves the battlefield, its controller loses 2 life.",
    );
    let mut engine = test_engine(3);
    let mut bear = permanent("borrowed-bear", "Creature — Bear", Vec::new(), "player-1");
    bear.controller = "player-2".to_string();
    engine.state.players[2].battlefield.push(bear);
    let mut aura = permanent("march", "Enchantment — Aura", rules, "player-0");
    aura.attached_to = Some("borrowed-bear".to_string());
    engine.state.players[0].battlefield.push(aura);
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("borrowed-bear", "destroyed")
        .expect("the enchanted creature leaves the battlefield");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[2].life, 18, "the last controller");
    assert_eq!(engine.state.players[1].life, 20, "not the owner");
    assert_eq!(
        engine.state.players[0].life, 20,
        "not the Aura's controller"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn batched_combat_damage_names_the_damaged_player() {
    // `one or more creatures you control deal combat damage to a player`:
    // the grouped record binds the damaged player, whom `that player` names.
    let rules = parsed_rules(
        "Enchantment",
        "Whenever one or more creatures you control deal combat damage to a player, that player loses 2 life.",
    );
    let rule = rule_of_kind(&rules, "triggeredAbility");
    let mut engine = test_engine(3);
    let source = permanent("raid", "Enchantment", rules.clone(), "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    admit(&mut engine);
    engine.put_triggered_ability_on_stack_with_decisions(
        &source,
        rule,
        BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String("player-2".to_string()),
        )]),
    );
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[2].life, 18, "the damaged player");
    assert_eq!(engine.state.players[1].life, 20);
    assert_eq!(engine.state.players[0].life, 20);
}
