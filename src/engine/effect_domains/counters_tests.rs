//! Runtime proofs of the counter placements the Oracle compiler lowers for
//! `your choice of a K1 counter or a K2 counter` (one placement chosen on
//! resolution, CR 608.2d) and `put its counters on X` (every kind of a bound
//! object's counters, as it last existed, CR 608.2h).
use super::super::super::*;
use super::supports_effect;

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

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{1}".to_string(),
        name: id.to_string(),
        power: Some("2".to_string()),
        rules: Vec::new(),
        toughness: Some("2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn engine_with_creature(id: &str) -> GameEngine {
    let mut filler = definition("counter-filler", "Basic Land - Forest");
    filler.power = None;
    filler.toughness = None;
    let mut engine = GameEngine::new(
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
    .expect("counter test setup is valid");
    let mut card = card_instance("player-0", &definition(id, "Creature - Bear"), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn stack_for(engine: &GameEngine, source: &str, decisions: BTreeMap<String, Value>) -> StackObject {
    let (player, index) = engine.permanent_position(source).expect("source exists");
    StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: engine.state.players[player].battlefield[index].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    }
}

fn counters_of(engine: &GameEngine, id: &str) -> BTreeMap<String, i32> {
    let (player, index) = engine.permanent_position(id).expect("permanent exists");
    engine.state.players[player].battlefield[index]
        .counters
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(counter, count)| (counter.clone(), *count))
        .collect()
}

fn chosen_counters(options: Value) -> Value {
    json!({
        "kind": "putChosenCounters",
        "permanent": { "kind": "self" },
        "options": options,
    })
}

#[test]
fn a_chosen_counter_kind_is_the_only_one_placed() {
    let mut engine = engine_with_creature("chooser");
    let effect = chosen_counters(json!([
        { "counter": "flying", "count": { "kind": "integer", "value": 1 } },
        { "counter": "hexproof", "count": { "kind": "integer", "value": 1 } },
    ]));
    assert_eq!(supports_effect(&effect), Some(true));
    let stack = stack_for(&engine, "chooser", BTreeMap::new());
    engine
        .execute_counters_domain_known(
            &effect,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the chosen placement resolves");
    assert_eq!(
        counters_of(&engine, "chooser"),
        BTreeMap::from([("flying".to_string(), 1)])
    );
}

#[test]
fn a_counter_kind_choice_needs_two_distinct_supported_options() {
    for options in [
        json!([{ "counter": "flying", "count": { "kind": "integer", "value": 1 } }]),
        json!([
            { "counter": "flying", "count": { "kind": "integer", "value": 1 } },
            { "counter": "flying", "count": { "kind": "integer", "value": 2 } },
        ]),
        json!([
            { "counter": "flying", "count": { "kind": "integer", "value": 1 } },
            { "counter": "charge", "count": { "kind": "unsupportedAmount" } },
        ]),
    ] {
        assert_eq!(supports_effect(&chosen_counters(options)), Some(false));
    }
}

#[test]
fn putting_an_event_permanents_counters_preserves_every_kind() {
    let mut engine = engine_with_creature("recipient");
    let effect = json!({
        "kind": "putSameCountersAs",
        "permanent": { "kind": "self" },
        "source": { "kind": "triggeringPermanent" },
    });
    assert_eq!(supports_effect(&effect), Some(true));
    let stack = stack_for(
        &engine,
        "recipient",
        BTreeMap::from([(
            "triggeringPermanentCounters".to_string(),
            json!({ "+1/+1": 2, "flying": 1 }),
        )]),
    );
    engine
        .execute_counters_domain_known(
            &effect,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the counters of the departed permanent are put");
    assert_eq!(
        counters_of(&engine, "recipient"),
        BTreeMap::from([("+1/+1".to_string(), 2), ("flying".to_string(), 1)])
    );
}
