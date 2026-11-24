//! Runtime proofs of the shared object query's set constraints and
//! superlatives at choice time: `with different names`, `with total mana
//! value N or less` (a constraint over the whole selection, never over one
//! candidate) and `with the greatest power among creatures they control`
//! (a ranking of the chooser's candidates, ties kept), plus the distinct
//! power count read outside a resolving stack object (Coven).
use super::super::super::*;

/// Records every offered selection and picks the first one.
struct Record {
    decision: &'static str,
    offered: Vec<Vec<String>>,
}

impl DecisionProvider for Record {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.offered = request
            .options
            .iter()
            .filter_map(|option| option.decisions.get(self.decision))
            .map(|selection| {
                selection
                    .as_array()
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        Ok(0)
    }
}

fn definition(name: &str, power: i64, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: name.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: mana_cost.to_string(),
        name: name.to_string(),
        power: Some(power.to_string()),
        rules: Vec::new(),
        toughness: Some("2".to_string()),
        type_line: "Creature - Bear".to_string(),
    }
}

fn engine() -> GameEngine {
    let mut filler = definition("set-filler", 0, "");
    filler.type_line = "Basic Land - Forest".to_string();
    filler.power = None;
    filler.toughness = None;
    GameEngine::new(
        GameSetup {
            opening_hand_size: 7,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 12],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        512,
    )
    .expect("set-constraint test setup is valid")
}

/// A creature on player 0's battlefield with an explicit instance id.
fn add(engine: &mut GameEngine, id: &str, name: &str, power: i64, mana_cost: &str) {
    let mut card = card_instance("player-0", &definition(name, power, mana_cost), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

/// `choose up to two creatures you control` with the given extra fields,
/// as the Oracle compiler lowers a player's choice among their permanents.
fn choice(minimum: i64, maximum: i64, extra: Value) -> Value {
    let mut effect = json!({
        "kind": "chooseCards",
        "id": "pick",
        "player": controller(),
        "from": {
            "kind": "eachPermanent",
            "player": controller(),
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        },
        "minimum": { "kind": "integer", "value": minimum },
        "maximum": { "kind": "integer", "value": maximum },
    });
    for (key, value) in extra.as_object().expect("extra fields").clone() {
        effect[key] = value;
    }
    effect
}

fn rule(effect: &Value) -> Value {
    json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [effect],
    })
}

fn offered(engine: &mut GameEngine, effect: &Value, source: &str) -> Vec<Vec<String>> {
    let rule = rule(effect);
    let (player, index) = engine.permanent_position(source).expect("source exists");
    let stack = StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: engine.state.players[player].battlefield[index].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let mut provider = Record {
        decision: "pick",
        offered: Vec::new(),
    };
    let mut decisions = BTreeMap::new();
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut provider,
        )
        .expect("the constrained choice resolves");
    let mut offered = provider.offered;
    // A single legal selection is made without asking: it is the offer.
    if offered.is_empty()
        && let Some(chosen) = decisions.get("pick").and_then(Value::as_array)
    {
        offered.push(
            chosen
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
        );
    }
    for selection in &mut offered {
        selection.sort();
    }
    offered.sort();
    offered
}

#[test]
fn a_distinct_names_choice_never_offers_two_cards_with_one_name() {
    let effect = choice(
        0,
        2,
        json!({ "setConstraints": [{ "kind": "distinctValues", "property": "name" }] }),
    );
    assert!(rule_is_executable(&rule(&effect)), "{effect}");
    let mut engine = engine();
    add(&mut engine, "bear-a", "Grizzly Bears", 2, "{1}{G}");
    add(&mut engine, "bear-b", "Grizzly Bears", 2, "{1}{G}");
    add(&mut engine, "elk", "Elk", 3, "{2}{G}");
    let offered = offered(&mut engine, &effect, "elk");
    let pair = vec!["bear-a".to_string(), "bear-b".to_string()];
    assert!(
        !offered.contains(&pair),
        "two Grizzly Bears share a name: {offered:?}"
    );
    // Every other selection of at most two remains legal.
    assert_eq!(offered.len(), 6, "{offered:?}");
}

#[test]
fn a_total_mana_value_choice_rejects_a_selection_over_the_bound() {
    let effect = choice(
        0,
        2,
        json!({ "setConstraints": [{
            "kind": "totalAtMost",
            "property": "manaValue",
            "value": { "kind": "integer", "value": 3 },
        }] }),
    );
    assert!(rule_is_executable(&rule(&effect)), "{effect}");
    let mut engine = engine();
    add(&mut engine, "one", "One", 1, "{G}");
    add(&mut engine, "two", "Two", 1, "{1}{G}");
    add(&mut engine, "three", "Three", 1, "{2}{G}");
    let offered = offered(&mut engine, &effect, "one");
    // Empty, three singles, and only {one, two} among the pairs.
    assert_eq!(offered.len(), 5, "{offered:?}");
    assert!(offered.contains(&vec!["one".to_string(), "two".to_string()]));
    assert!(!offered.contains(&vec!["one".to_string(), "three".to_string()]));
}

#[test]
fn distinct_and_total_constraints_compose_over_one_selection() {
    // `with different mana values and total mana value 3 or less`.
    let effect = choice(
        2,
        2,
        json!({ "setConstraints": [
            { "kind": "distinctValues", "property": "manaValue" },
            { "kind": "totalAtMost", "property": "manaValue", "value": 3 },
        ] }),
    );
    assert!(rule_is_executable(&rule(&effect)), "{effect}");
    let mut engine = engine();
    add(&mut engine, "one-a", "One A", 1, "{G}");
    add(&mut engine, "one-b", "One B", 1, "{G}");
    add(&mut engine, "two", "Two", 1, "{1}{G}");
    let offered = offered(&mut engine, &effect, "two");
    assert_eq!(
        offered,
        vec![
            vec!["one-a".to_string(), "two".to_string()],
            vec!["one-b".to_string(), "two".to_string()],
        ]
    );
}

#[test]
fn a_superlative_with_a_tie_lets_the_player_pick_among_the_tied() {
    // `sacrifices a creature with the greatest power among creatures they
    // control`: both 3-power creatures are candidates, the 1-power one is not.
    let effect = choice(
        1,
        1,
        json!({ "extremum": { "property": "power", "direction": "greatest" } }),
    );
    assert!(rule_is_executable(&rule(&effect)), "{effect}");
    let mut engine = engine();
    add(&mut engine, "big-a", "Big A", 3, "{2}{G}");
    add(&mut engine, "big-b", "Big B", 3, "{2}{G}");
    add(&mut engine, "small", "Small", 1, "{G}");
    let offered = offered(&mut engine, &effect, "small");
    assert_eq!(
        offered,
        vec![vec!["big-a".to_string()], vec!["big-b".to_string()]]
    );
}

#[test]
fn the_least_mana_value_superlative_ranks_by_mana_value() {
    let effect = choice(
        1,
        1,
        json!({ "extremum": { "property": "manaValue", "direction": "least" } }),
    );
    assert!(rule_is_executable(&rule(&effect)), "{effect}");
    let mut engine = engine();
    add(&mut engine, "cheap", "Cheap", 5, "{G}");
    add(&mut engine, "dear", "Dear", 1, "{4}{G}");
    let offered = offered(&mut engine, &effect, "dear");
    assert_eq!(offered, vec![vec!["cheap".to_string()]]);
}

#[test]
fn unknown_set_constraints_and_extrema_are_refused() {
    let refused = [
        json!({ "setConstraints": [{ "kind": "distinctValues", "property": "color" }] }),
        json!({ "setConstraints": [{ "kind": "totalAtMost", "property": "manaValue" }] }),
        json!({ "setConstraints": [] }),
        json!({ "setConstraints": [{ "kind": "sameOwner" }] }),
        json!({ "extremum": { "property": "loyalty", "direction": "greatest" } }),
    ];
    for extra in refused {
        assert!(
            !rule_is_executable(&rule(&choice(0, 1, extra.clone()))),
            "{extra}"
        );
    }
}

#[test]
fn a_declared_target_set_is_checked_against_its_set_constraint() {
    let mut engine = engine();
    add(&mut engine, "two", "Two", 1, "{1}{G}");
    add(&mut engine, "three", "Three", 1, "{2}{G}");
    let targets = |ids: &[&str]| {
        ids.iter()
            .map(|id| TargetRef::Permanent {
                instance_id: (*id).to_string(),
            })
            .collect::<Vec<_>>()
    };
    let total = json!({ "kind": "totalAtMost", "property": "manaValue", "value": 4 });
    assert!(engine.target_selection_satisfies_set_constraint(&targets(&["two"]), &total));
    assert!(!engine.target_selection_satisfies_set_constraint(&targets(&["two", "three"]), &total));
    // A player has no mana value: the constraint fails rather than skipping it.
    let with_player = vec![TargetRef::Player {
        player_id: "player-1".to_string(),
    }];
    assert!(!engine.target_selection_satisfies_set_constraint(&with_player, &total));
}

#[test]
fn a_distinct_power_count_is_read_outside_a_resolving_object() {
    // Coven: `if you control three or more creatures with different powers`.
    let mut engine = engine();
    add(&mut engine, "a", "A", 2, "{G}");
    add(&mut engine, "b", "B", 2, "{G}");
    add(&mut engine, "c", "C", 4, "{G}");
    let count = json!({
        "kind": "countDistinctPowers",
        "player": controller(),
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    assert_eq!(engine.numeric_expression(&count, "player-0"), Some(2));
    // A source exclusion needs the source: no value here, never a guess.
    let mut excluding = count.clone();
    excluding["excludeSource"] = json!(true);
    assert_eq!(engine.numeric_expression(&excluding, "player-0"), None);
}

#[test]
fn the_variable_mana_symbol_atom_reads_the_printed_cost() {
    let with_x = definition("Hydra", 0, "{X}{G}{G}");
    let without_x = definition("Bear", 2, "{1}{G}");
    let atom = json!({ "kind": "manaCostContainsX" });
    assert!(matches_card_filter(&with_x, &atom));
    assert!(!matches_card_filter(&without_x, &atom));
}
