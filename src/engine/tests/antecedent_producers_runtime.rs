//! Real Oracle text -> primitive IR -> engine behavior for the antecedent
//! producers of batch 10: the card a chosen cost paid with, read by its
//! printed characteristics recorded when the cost was paid (CR 601.2h,
//! 602.2b, 608.2h), and a just-affected collection named again after a gain
//! of control (CR 608.2c).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{B}".to_string()),
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

/// A card definition with a printed mana cost and power.
fn card(id: &str, type_line: &str, mana_cost: &str, power: &str) -> CardDefinition {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.power = Some(power.to_string());
    definition.toughness = Some("1".to_string());
    definition
}

fn stack_object(card: CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:antecedent-probe".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: rule["kind"].as_str().map(str::to_string),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn resolve(engine: &mut GameEngine, stack: &StackObject) {
    let effects = stack.ability_rule.as_ref().expect("a rule")["effects"].clone();
    // Resolution starts from the decisions the stack object recorded when
    // its costs were paid, as the engine's stack resolution does.
    execute_effects_at_resolution_start(
        engine,
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut stack.decisions.clone(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the compiled program executes");
}

/// Every cost object under `value` (activation costs and additional costs
/// alike) that asks to record one of its card's characteristics.
fn recording_costs(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            if ["bindPowerAs", "bindToughnessAs", "bindManaValueAs"]
                .iter()
                .any(|field| map.contains_key(*field))
            {
                found.push(value.clone());
            }
            map.values().for_each(|child| recording_costs(child, found));
        }
        Value::Array(items) => items.iter().for_each(|child| recording_costs(child, found)),
        _ => {}
    }
}

/// The chosen card id a card cost names.
fn chosen_card_id(cost: &Value) -> String {
    cost["card"]["id"]
        .as_str()
        .or_else(|| cost["object"]["id"].as_str())
        .expect("a chosen card")
        .to_string()
}

const VOLRATH: &str = "{1}{B}, Discard a creature card: Volrath gets +X/+X until end of turn, where X is the discarded card's mana value.";

/// Volrath the Fallen discarding a creature card of `mana_cost`: the power
/// Volrath has once the ability resolved.
fn volrath_power(mana_cost: &str) -> i32 {
    let rule = parsed_rules(
        "Volrath the Fallen",
        "Legendary Creature — Phyrexian Shapeshifter",
        VOLRATH,
    )
    .remove(0);
    let discard = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|cost| value_kind(cost) == Some("discardCard"))
        .expect("a discard cost");
    // Card composition proved the producing cost and asked it to record the
    // discarded card's mana value.
    assert_eq!(discard["bindManaValueAs"], "discardedCardManaValue");
    let mut engine = test_engine(2);
    let volrath = test_instance(
        "volrath",
        card(
            "volrath",
            "Legendary Creature — Phyrexian",
            "{3}{B}{B}{B}",
            "6",
        ),
        "player-0",
    );
    let discarded = test_instance(
        "discarded",
        card("discarded", "Creature — Horror", mana_cost, "3"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(volrath.clone());
    // The payment already put the chosen card into its owner's graveyard.
    engine.state.players[0].graveyard.push(discarded);
    let targets = BTreeMap::from([(
        chosen_card_id(discard),
        TargetRef::Card {
            instance_id: "discarded".to_string(),
        },
    )]);
    let mut stack = stack_object(volrath, &rule);
    engine.record_cost_paid_objects(&rule["costs"], &targets, &mut stack.decisions);
    resolve(&mut engine, &stack);
    let (player, index) = engine
        .permanent_position("volrath")
        .expect("on battlefield");
    engine.current_power(&engine.state.players[player].battlefield[index])
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn volrath_gets_the_discarded_cards_mana_value() {
    assert_eq!(volrath_power("{2}{B}{B}"), 10);
    // Sibling: the same activation with a cheaper discarded card.
    assert_eq!(volrath_power("{G}"), 7);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn living_destiny_gains_the_revealed_additional_cost_cards_mana_value() {
    let rules = parsed_rules(
        "Living Destiny",
        "Instant",
        "As an additional cost to cast this spell, reveal a creature card from your hand.\nYou gain life equal to the revealed card's mana value.",
    );
    let mut costs = Vec::new();
    rules
        .iter()
        .for_each(|rule| recording_costs(rule, &mut costs));
    let [reveal] = costs.as_slice() else {
        panic!("exactly one recording cost: {costs:#?}");
    };
    assert_eq!(value_kind(reveal), Some("revealCard"));
    assert_eq!(reveal["bindManaValueAs"], "revealedCostCardManaValue");
    let spell = rules
        .iter()
        .find(|rule| {
            rule["effects"]
                .as_array()
                .is_some_and(|effects| !effects.is_empty())
        })
        .expect("the spell's effects");
    let mut engine = test_engine(2);
    let revealed = test_instance(
        "revealed",
        card("revealed", "Creature — Beast", "{4}{G}{G}", "6"),
        "player-0",
    );
    // A revealed card stays in its owner's hand (CR 701.16a).
    engine.state.players[0].hand.push(revealed);
    let targets = BTreeMap::from([(
        chosen_card_id(reveal),
        TargetRef::Card {
            instance_id: "revealed".to_string(),
        },
    )]);
    let source = test_instance("destiny", test_definition("destiny", "Instant"), "player-0");
    let mut stack = stack_object(source, spell);
    engine.record_cost_paid_card_properties(
        &Value::Array(vec![reveal.clone()]),
        &targets,
        &mut stack.decisions,
    );
    assert_eq!(stack.decisions["revealedCostCardManaValue"], 6);
    resolve(&mut engine, &stack);
    assert_eq!(engine.state.players[0].life, 26);
}

const CORPSE_LUNGE: &str = "As an additional cost to cast this spell, exile a creature card from your graveyard.\nCorpse Lunge deals damage equal to the exiled card's power to target creature.";

/// Corpse Lunge exiling `exiled` (already moved to exile by the payment) at
/// a 2/10 creature of player 1: the resolution's result and the damage the
/// creature has marked.
fn corpse_lunge(exiled: CardDefinition) -> (Result<(), EngineError>, i32) {
    let rules = parsed_rules("Corpse Lunge", "Instant", CORPSE_LUNGE);
    let mut costs = Vec::new();
    rules
        .iter()
        .for_each(|rule| recording_costs(rule, &mut costs));
    let [exile] = costs.as_slice() else {
        panic!("exactly one recording cost: {costs:#?}");
    };
    assert_eq!(value_kind(exile), Some("exileObject"));
    let binding = exile["bindPowerAs"]
        .as_str()
        .expect("the power is recorded");
    let spell = rules
        .iter()
        .find(|rule| {
            rule["effects"]
                .as_array()
                .is_some_and(|effects| !effects.is_empty())
        })
        .expect("the spell's effects");
    let target_id = spell["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|decision| value_kind(decision) == Some("chooseTargets"))
        .and_then(|decision| decision["id"].as_str())
        .expect("one target creature")
        .to_string();
    let mut engine = test_engine(2);
    engine.state.players[0]
        .exile
        .push(test_instance("exiled", exiled, "player-0"));
    let mut victim = card("victim", "Creature — Wall", "{1}", "2");
    victim.toughness = Some("10".to_string());
    engine.state.players[1]
        .battlefield
        .push(test_instance("victim", victim, "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = test_instance("lunge", test_definition("lunge", "Instant"), "player-0");
    let mut stack = stack_object(source, spell);
    engine.record_cost_paid_card_properties(
        &Value::Array(vec![exile.clone()]),
        &BTreeMap::from([(
            chosen_card_id(exile),
            TargetRef::Card {
                instance_id: "exiled".to_string(),
            },
        )]),
        &mut stack.decisions,
    );
    assert!(
        stack.decisions.contains_key(binding),
        "{:#?}",
        stack.decisions
    );
    stack.targets.insert(
        target_id,
        TargetRef::Permanent {
            instance_id: "victim".to_string(),
        },
    );
    let effects = spell["effects"].clone();
    let result = execute_effects_at_resolution_start(
        &mut engine,
        &effects,
        &stack,
        &mut BTreeMap::new(),
        &mut stack.decisions.clone(),
        &mut EmeritusDecisionProvider,
    );
    let damage = engine
        .permanent_position("victim")
        .map(|(player, index)| engine.state.players[player].battlefield[index].damage_marked)
        .unwrap_or_default();
    (result, damage)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn corpse_lunge_reads_a_characteristic_defining_power_in_exile() {
    // A printed power is recorded when the cost is paid.
    let (result, damage) = corpse_lunge(card("printed", "Creature — Horror", "{3}", "3"));
    result.expect("a printed power resolves");
    assert_eq!(damage, 3);
    // A `*` power is defined by the card's own ability in exile (CR 604.3),
    // never dealt as 0.
    let mut defined = card("defined", "Creature — Lhurgoyf", "{2}{G}", "*");
    defined.toughness = Some("1+*".to_string());
    defined.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "characteristicDefining": true,
        "modifiers": [{
            "kind": "setBasePowerToughness",
            "objects": { "kind": "self" },
            "layer": "7a",
            "power": { "kind": "integer", "value": 5 },
            "toughness": { "kind": "integer", "value": 6 },
        }],
    })];
    let (result, damage) = corpse_lunge(defined);
    result.expect("a defined power resolves");
    assert_eq!(damage, 5);
    // Near miss: a `*` power with no ability the engine evaluates is refused
    // explicitly instead of dealing 0 damage.
    let (result, damage) = corpse_lunge(card("undefined", "Creature — Avatar", "{3}{G}", "*"));
    let error = result.expect_err("an undetermined power is refused");
    assert!(error.to_string().contains("has no value"), "{error}");
    assert_eq!(damage, 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn insurrection_hastes_the_creatures_it_untapped_and_took() {
    let rule = parsed_rules(
        "Insurrection",
        "Sorcery",
        "Untap all creatures and gain control of them until end of turn. They gain haste until end of turn.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    for (player, id) in [(0, "own"), (1, "theirs")] {
        let mut creature = test_instance(
            id,
            test_definition(id, "Creature — Bear"),
            &format!("player-{player}"),
        );
        creature.tapped = true;
        engine.state.players[player].battlefield.push(creature);
    }
    let source = test_instance(
        "insurrection",
        test_definition("insurrection", "Sorcery"),
        "player-0",
    );
    let stack = stack_object(source, &rule);
    resolve(&mut engine, &stack);
    for id in ["own", "theirs"] {
        let (player, index) = engine.permanent_position(id).expect("on battlefield");
        let permanent = engine.state.players[player].battlefield[index].clone();
        assert_eq!(permanent.controller, "player-0", "{id}");
        assert!(!permanent.tapped, "{id}");
        assert!(engine.permanent_has_keyword(&permanent, "haste"), "{id}");
    }
}
