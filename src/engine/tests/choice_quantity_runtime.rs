//! Runtime proof of the shared player-chosen cardinality slot: a cost whose
//! number the payer chooses while paying (CR 601.2h via 602.2b, all or
//! nothing per CR 118.3) and a resolution selection whose size the acting
//! player picks within a range (CR 608.2d). Every rule below is the
//! production compiler's output for Oracle text.
use super::*;

/// Picks the resolution option that selects exactly `count` objects and
/// records the bounds the engine offered.
struct SelectCount {
    count: usize,
    offered: Option<(usize, usize)>,
}

impl DecisionProvider for SelectCount {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(DecisionChoice::CardSelection {
            minimum, maximum, ..
        }) = &request.choice
        {
            self.offered = Some((*minimum, *maximum));
        }
        request
            .options
            .iter()
            .position(|option| {
                option
                    .decisions
                    .values()
                    .any(|value| value.as_array().is_some_and(|ids| ids.len() == self.count))
            })
            .ok_or_else(|| EngineError::new("no option selects the wanted number"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: String::new(),
        power: Some("1".to_string()),
        toughness: Some("1".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        909,
    )
    .unwrap();
    engine.state.step = GameStep::PrecombatMain;
    engine
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

const STORAGE: &str = "{T}, Remove any number of storage counters from this land: Add {C} for each storage counter removed this way.";

/// A storage land with `counters` storage counters and only its storage
/// ability, on player 0's battlefield.
fn storage_land(engine: &mut GameEngine, counters: i32) -> (Value, String) {
    let rule = rules("Storage Probe", "Land", "", STORAGE).remove(0);
    let mut land = definition("storage-probe", "Land");
    land.power = None;
    land.toughness = None;
    land.rules = vec![rule.clone()];
    let mut instance = card_instance("player-0", &land, 700);
    instance.counters.insert("storage".to_string(), counters);
    let id = instance.instance_id.clone();
    engine.state.players[0].battlefield = vec![instance];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    (rule, id)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_storage_land_declares_each_payable_number_and_pays_exactly_it() {
    let mut engine = engine();
    let (rule, id) = storage_land(&mut engine, 3);
    // The payer-chosen number is bounded by the source's storage counters
    // when the activation is declared: zero through three.
    let mut offered = engine
        .declarations_for_activated_rule(&rule, "player-0", &id)
        .expect("declarations")
        .into_iter()
        .filter_map(|declaration| declaration.decisions.get("costChosenAmount").cloned())
        .filter_map(|value| value.as_i64())
        .collect::<Vec<_>>();
    offered.sort_unstable();
    offered.dedup();
    assert_eq!(offered, vec![0, 1, 2, 3]);
    let activation = CanonicalManaAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "storage-probe".into(),
        mana_symbols: Some(vec!["C".into(), "C".into()]),
    };
    let chosen = BTreeMap::from([("costChosenAmount".to_string(), Value::from(2))]);
    engine
        .activate_canonical_mana_ability_with_decisions(
            &activation,
            &BTreeMap::new(),
            &chosen,
            &mut SelectCount {
                count: 0,
                offered: None,
            },
        )
        .expect("the storage ability activates");
    assert_eq!(engine.state.players[0].mana_pool.len(), 2);
    assert!(
        engine.state.players[0]
            .mana_pool
            .iter()
            .all(|mana| mana.symbol == "C")
    );
    assert_eq!(
        engine.state.players[0].battlefield[0]
            .counters
            .get("storage"),
        Some(&1)
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn removing_zero_counters_is_a_legal_payment() {
    let mut engine = engine();
    let (rule, _) = storage_land(&mut engine, 0);
    let cost = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|cost| value_kind(cost) == Some("removeCounters"))
        .expect("a counter-removal cost");
    let source = engine.state.players[0].battlefield[0].clone();
    let declared =
        |amount: i64| BTreeMap::from([("costChosenAmount".to_string(), Value::from(amount))]);
    assert!(GameEngine::remove_counter_cost_is_payable(
        &source,
        cost,
        &declared(0)
    ));
    // More counters than the source has are never paid (CR 118.3).
    assert!(!GameEngine::remove_counter_cost_is_payable(
        &source,
        cost,
        &declared(1)
    ));
}

fn stack_object(card: CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:quantity".to_string(),
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

/// Resolves `text` with three artifacts and one creature on player 0's
/// battlefield, the provider selecting `count` permanents.
fn resolve_with_selection(text: &str, count: usize) -> (GameEngine, SelectCount) {
    let rule = rules("Quantity Probe", "Sorcery", "{2}{B}", text).remove(0);
    let mut engine = engine();
    let spell = card_instance("player-0", &definition("quantity-probe", "Sorcery"), 800);
    engine.state.players[0].battlefield = (0..3)
        .map(|index| {
            card_instance(
                "player-0",
                &definition(&format!("relic-{index}"), "Artifact"),
                801 + index,
            )
        })
        .chain((0..3).map(|index| {
            card_instance(
                "player-0",
                &definition(&format!("soldier-{index}"), "Creature — Human Soldier"),
                811 + index,
            )
        }))
        .collect();
    let stack = stack_object(spell, &rule);
    let mut provider = SelectCount {
        count,
        offered: None,
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the compiled program executes");
    (engine, provider)
}

fn remaining(engine: &GameEngine, type_line: &str) -> usize {
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.type_line.contains(type_line))
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn any_number_sacrifices_the_chosen_artifacts_and_counts_them() {
    const TEXT: &str =
        "Sacrifice any number of artifacts. Draw a card for each artifact sacrificed this way.";
    let (engine, provider) = resolve_with_selection(TEXT, 2);
    // Zero through every matching artifact (CR 608.2d).
    assert_eq!(provider.offered, Some((0, 3)));
    assert_eq!(remaining(&engine, "Artifact"), 1);
    assert_eq!(remaining(&engine, "Creature"), 3);
    assert_eq!(engine.state.players[0].hand.len(), 2);
    // Choosing none is legal and sacrifices and draws nothing.
    let (engine, _) = resolve_with_selection(TEXT, 0);
    assert_eq!(remaining(&engine, "Artifact"), 3);
    assert!(engine.state.players[0].hand.is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn up_to_n_bounds_the_selection_and_that_many_restates_it() {
    let (engine, provider) = resolve_with_selection(
        "Sacrifice up to two creatures, then draw that many cards.",
        2,
    );
    assert_eq!(provider.offered, Some((0, 2)));
    assert_eq!(remaining(&engine, "Creature"), 1);
    assert_eq!(engine.state.players[0].hand.len(), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_permanent_chosen_at_random_is_picked_by_the_game_and_the_rest_destroyed() {
    let rule = rules(
        "Last One Standing",
        "Sorcery",
        "{1}{B}{R}",
        "Choose a creature at random, then destroy the rest.",
    )
    .remove(0);
    let mut engine = engine();
    let spell = card_instance("player-0", &definition("last-one-standing", "Sorcery"), 900);
    engine.state.players[0].battlefield = (0..2)
        .map(|index| {
            card_instance(
                "player-0",
                &definition(&format!("ally-{index}"), "Creature — Human"),
                901 + index,
            )
        })
        .collect();
    engine.state.players[1].battlefield = (0..2)
        .map(|index| {
            card_instance(
                "player-1",
                &definition(&format!("foe-{index}"), "Creature — Orc"),
                911 + index,
            )
        })
        .collect();
    let stack = stack_object(spell, &rule);
    // No player decides a random pick: a provider asked anything fails.
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _state: &GameState,
            request: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new(format!(
                "a random pick asked a player: {}",
                request.id
            )))
        }
    }
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut NoChoice,
    )
    .expect("the compiled program executes");
    let survivors = engine
        .state
        .players
        .iter()
        .map(|player| player.battlefield.len())
        .sum::<usize>();
    assert_eq!(survivors, 1);
}
