//! Runtime proof of definite descriptions as operands
//! (`described_object_anchor.rs`): a relation anchored on the object a cost
//! paid with reads the record its paying cost wrote (CR 601.2h, 608.2h), a
//! static cast prohibition anchored on the source's linked exile reads the
//! cards it exiled (CR 607.2a), and a numeric read of `the creature
//! sacrificed this way` is that creature's last known information. Every
//! rule below is the production compiler's output for Oracle text.
use super::*;

struct FirstOption;
impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: mana_cost.to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", "{1}"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2024,
    )
    .unwrap()
}

/// The canonical, executable rules of one Oracle text.
fn rules(name: &str, type_line: &str, mana_cost: &str, oracle_text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed
        .abilities
        .into_iter()
        .map(|ability| {
            let rule = ability.rule.expect("canonical rule");
            assert!(rule_is_executable(&rule), "{rule:#}");
            rule
        })
        .collect()
}

fn stack_object(card: CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:described".to_string(),
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

fn run(engine: &mut GameEngine, stack: &StackObject) {
    let effects = stack.ability_rule.as_ref().unwrap()["effects"].clone();
    execute_effects_at_resolution_start(
        engine,
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut FirstOption,
    )
    .expect("the compiled program executes");
}

/// The cost list of `rules` that sacrifices a permanent (an activation's
/// costs or a spell's additional costs), with its sacrifice cost.
fn sacrifice_costs(rules: &[Value]) -> (Value, Value) {
    rules
        .iter()
        .flat_map(|rule| {
            [
                rule["costs"].clone(),
                rule["declaration"]["additionalCosts"].clone(),
            ]
        })
        .find_map(|costs| {
            let sacrifice = costs
                .as_array()?
                .iter()
                .find(|cost| value_kind(cost) == Some("sacrificePermanent"))?
                .clone();
            Some((costs, sacrifice))
        })
        .expect("a sacrifice cost")
}

fn on_battlefield(engine: &GameEngine, id: &str) -> bool {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .any(|permanent| permanent.instance_id == id)
}

/// Pays the sacrifice cost of `rules` with `victim` (recording its object
/// as the composition asked), then resolves the program that reads it.
fn sacrifice_and_resolve(
    engine: &mut GameEngine,
    rules: &[Value],
    source: CardInstance,
    victim_id: &str,
) {
    let (costs, sacrifice) = sacrifice_costs(rules);
    // Card composition proved the producing cost and asked it to record
    // its object for the relation anchor.
    assert_eq!(sacrifice["bindObjectAs"], "sacrificedPermanentObject");
    let targets = BTreeMap::from([(
        sacrifice["permanent"]["id"].as_str().unwrap().to_string(),
        TargetRef::Permanent {
            instance_id: victim_id.to_string(),
        },
    )]);
    let reader = rules
        .iter()
        // The cost declaration's own rule has an empty program.
        .find(|rule| {
            rule["effects"]
                .as_array()
                .is_some_and(|effects| !effects.is_empty())
        })
        .expect("the reading program");
    let mut stack = stack_object(source, reader);
    engine.record_cost_paid_objects(&costs, &targets, &mut stack.decisions);
    assert!(stack.decisions.contains_key("sacrificedPermanentObject"));
    engine
        .move_permanent_to_graveyard(victim_id, "sacrificed")
        .expect("the sacrifice");
    run(engine, &stack);
}

/// Endemic Plague: the creatures sharing a creature type with the creature
/// sacrificed as the additional cost are destroyed; the others survive.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn endemic_plague_destroys_by_the_sacrificed_creatures_recorded_types() {
    let rules = rules(
        "Endemic Plague",
        "Sorcery",
        "{3}{B}",
        "As an additional cost to cast this spell, sacrifice a creature.\nDestroy all creatures that share a creature type with the sacrificed creature. They can't be regenerated.",
    );
    let mut engine = engine();
    let source = card_instance("player-0", &definition("plague", "Sorcery", "{3}{B}"), 950);
    let victim = card_instance(
        "player-0",
        &definition("victim", "Creature — Goblin Warrior", "{1}{R}"),
        951,
    );
    let kin = card_instance(
        "player-1",
        &definition("kin", "Creature — Goblin Shaman", "{R}"),
        952,
    );
    let stranger = card_instance(
        "player-1",
        &definition("stranger", "Creature — Elf Druid", "{G}"),
        953,
    );
    let (victim_id, kin_id, stranger_id) = (
        victim.instance_id.clone(),
        kin.instance_id.clone(),
        stranger.instance_id.clone(),
    );
    engine.state.players[0].battlefield.push(victim);
    engine.state.players[1].battlefield.push(kin);
    engine.state.players[1].battlefield.push(stranger);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    sacrifice_and_resolve(&mut engine, &rules, source, &victim_id);
    assert!(!on_battlefield(&engine, &kin_id), "a Goblin shares a type");
    assert!(on_battlefield(&engine, &stranger_id), "an Elf shares none");
}

/// Sanguine Praetor (activated): the mana value is the sacrificed
/// creature's as its cost recorded it; the sibling relation shares no
/// characteristic with the graveyard card's new object.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn sanguine_praetor_destroys_by_the_sacrificed_creatures_mana_value() {
    let rules = rules(
        "Sanguine Praetor",
        "Creature — Phyrexian Praetor",
        "{6}{B}{B}",
        "{B}, Sacrifice a creature: Destroy each creature with the same mana value as the sacrificed creature.",
    );
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("praetor", "Creature — Phyrexian Praetor", "{6}{B}{B}"),
        960,
    );
    let victim = card_instance(
        "player-0",
        &definition("victim", "Creature — Human", "{1}{W}"),
        961,
    );
    let same = card_instance(
        "player-1",
        &definition("same", "Creature — Elf", "{G}{G}"),
        962,
    );
    let other = card_instance(
        "player-1",
        &definition("other", "Creature — Elf", "{2}{G}"),
        963,
    );
    let (victim_id, same_id, other_id, source_id) = (
        victim.instance_id.clone(),
        same.instance_id.clone(),
        other.instance_id.clone(),
        source.instance_id.clone(),
    );
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].battlefield.push(victim);
    engine.state.players[1].battlefield.push(same);
    engine.state.players[1].battlefield.push(other);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    sacrifice_and_resolve(&mut engine, &rules, source, &victim_id);
    assert!(!on_battlefield(&engine, &same_id), "mana value 2 is shared");
    assert!(on_battlefield(&engine, &other_id), "mana value 3 is not");
    assert!(on_battlefield(&engine, &source_id), "mana value 8 is not");
}

/// Ixalan's Binding: the static prohibition reads the card its linked
/// exile recorded, for opponents only; another name, and the same name
/// once nothing is exiled, are castable.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_static_prohibition_reads_the_linked_exiled_cards_name() {
    let text = "When this enchantment enters, exile target nonland permanent an opponent controls until this enchantment leaves the battlefield.\nYour opponents can't cast spells with the same name as the exiled card.";
    let mut binding = definition("Ixalan's Binding", "Enchantment", "{3}{W}");
    binding.rules = rules("Ixalan's Binding", "Enchantment", "{3}{W}", text);
    let mut engine = engine();
    let source = card_instance("player-0", &binding, 970);
    let bear = definition("Grizzly Bears", "Creature — Bear", "{1}{G}");
    let exiled = card_instance("player-1", &bear, 971);
    let exiled_id = exiled.instance_id.clone();
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[1].exile.push(exiled);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let elves = definition("Llanowar Elves", "Creature — Elf Druid", "{G}");
    // Nothing exiled with the source yet: nothing shares its name.
    assert!(!engine.static_card_cast_prohibited("player-1", &bear));
    engine
        .register_linked_exiled_permanent(&source, &exiled_id, false, &bear)
        .expect("the linked exile");
    assert!(engine.static_card_cast_prohibited("player-1", &bear));
    assert!(!engine.static_card_cast_prohibited("player-0", &bear));
    assert!(!engine.static_card_cast_prohibited("player-1", &elves));
}

/// `the toughness of the creature sacrificed this way`: the earlier
/// action's result, read as last known information once it left.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_creature_sacrificed_this_way_is_read_as_last_known_information() {
    let rule = rules(
        "Toll Probe",
        "Sorcery",
        "{2}{B}",
        "Target player sacrifices a creature. You gain life equal to the toughness of the creature sacrificed this way.",
    )
    .remove(0);
    let mut engine = engine();
    let source = card_instance("player-0", &definition("toll", "Sorcery", "{2}{B}"), 980);
    let mut wall = definition("wall", "Creature — Wall", "{1}{W}");
    wall.power = Some("0".to_string());
    wall.toughness = Some("5".to_string());
    let victim = card_instance("player-1", &wall, 981);
    engine.state.players[1].battlefield.push(victim);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let mut stack = stack_object(source, &rule);
    let target = rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|decision| decision["id"].as_str())
        .next()
        .expect("the declared player target")
        .to_string();
    stack.targets.insert(
        target,
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
    );
    run(&mut engine, &stack);
    assert!(engine.state.players[1].battlefield.is_empty());
    assert_eq!(engine.state.players[0].life, 25);
}
