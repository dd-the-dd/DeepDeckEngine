//! Runtime proof of an action operand chosen on resolution (CR 608.2d), as
//! `effects::chosen_operand` lowers it: the acting player chooses among the
//! candidates while the action resolves, so the action never targets and
//! cannot fizzle (CR 115.1, 608.2b), and the action and a later anaphor read
//! the same bound object. The IR below is the lowered program of `Put a +1/+1
//! counter on a creature you control. It gains double strike until end of
//! turn.` and of `attach it to a creature you control`.
use super::*;

#[derive(Default)]
struct LastChoice {
    players: Vec<String>,
}

impl DecisionProvider for LastChoice {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        Ok(request.options.len().saturating_sub(1))
    }
}

fn controller() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn creature() -> Value {
    json!({"kind": "cardTypeContains", "value": "Creature"})
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn position() -> (GameEngine, StackObject) {
    let engine = GameEngine::new(
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
        2024,
    )
    .unwrap();
    let card = card_instance("player-0", &definition("source", "Sorcery"), 9999);
    let stack = StackObject {
        id: "chosen-operand".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn add(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) -> String {
    let index = 1000 + engine.state.players[player].battlefield.len() + player * 100;
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    card.controller = format!("player-{player}");
    let id = card.instance_id.clone();
    engine.state.players[player].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    id
}

fn choose_one_creature_you_control() -> Vec<Value> {
    vec![
        json!({"kind": "choosePermanents", "id": "effect0:choosePermanents", "player": controller(),
            "candidates": {"kind": "permanents", "where": creature(), "controller": controller()},
            "minimum": {"kind": "integer", "value": 1}, "maximum": {"kind": "integer", "value": 1}}),
        json!({"kind": "bind", "id": "effect0:chosenObjects",
            "value": {"kind": "decisionResult", "decisionId": "effect0:choosePermanents"}}),
    ]
}

fn chosen() -> Value {
    json!({"kind": "singleBoundObject", "binding": "effect0:chosenObjects"})
}

fn run(engine: &mut GameEngine, stack: &StackObject, effects: &Value) -> LastChoice {
    assert!(
        effects.as_array().unwrap().iter().all(effect_supported),
        "{effects:#?}"
    );
    let mut provider = LastChoice::default();
    engine
        .execute_effects(
            effects,
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("the lowered program executes");
    provider
}

#[test]
fn the_counter_recipient_is_chosen_on_resolution_among_the_controllers_creatures() {
    let (mut engine, stack) = position();
    let bear = add(&mut engine, 0, "bear", "Creature");
    let ogre = add(&mut engine, 0, "ogre", "Creature");
    add(&mut engine, 0, "relic", "Artifact");
    let wolf = add(&mut engine, 1, "wolf", "Creature");
    let mut program = choose_one_creature_you_control();
    program.push(json!({"kind": "putCounters", "permanent": chosen(),
        "counter": "+1/+1", "count": {"kind": "integer", "value": 1}}));
    let provider = run(&mut engine, &stack, &Value::Array(program));
    // The controller chose while the spell resolved, among their own two
    // creatures only (no declaration exists that could fizzle).
    assert_eq!(provider.players, ["player-0"]);
    let counters = |id: &str| {
        engine
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .find(|card| card.instance_id == id)
            .and_then(|card| card.counters.get("+1/+1").copied())
            .unwrap_or(0)
    };
    assert_eq!(counters(&bear) + counters(&ogre), 1);
    assert_eq!(counters(&wolf), 0);
}

#[test]
fn an_empty_candidate_set_resolves_without_a_choice_or_a_fizzle() {
    let (mut engine, stack) = position();
    add(&mut engine, 0, "relic", "Artifact");
    let wolf = add(&mut engine, 1, "wolf", "Creature");
    let mut program = choose_one_creature_you_control();
    program.push(json!({"kind": "putCounters", "permanent": chosen(),
        "counter": "+1/+1", "count": {"kind": "integer", "value": 1}}));
    run(&mut engine, &stack, &Value::Array(program));
    // CR 609.3: nothing to choose, nothing gets the counter; the opponent's
    // creature was never a candidate.
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .find(|card| card.instance_id == wolf)
            .is_some_and(|card| card.counters.is_empty())
    );
}

#[test]
fn the_equipment_is_attached_to_the_chosen_creature() {
    let (mut engine, stack) = position();
    let sword = add(&mut engine, 0, "sword", "Artifact \u{2014} Equipment");
    let bear = add(&mut engine, 0, "bear", "Creature");
    add(&mut engine, 1, "wolf", "Creature");
    // `Search ... put it onto the battlefield, attach it to a creature you
    // control`: the moved Equipment is a bound object the search wrote; the
    // binding is installed here as the search would leave it.
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "movedOntoBattlefield".to_string(),
        RuntimeBinding::Objects(vec![sword.clone()]),
    );
    let mut program = choose_one_creature_you_control();
    program.push(json!({"kind": "attachPermanent",
        "attachment": {"kind": "singleBoundObject", "binding": "movedOntoBattlefield"},
        "to": chosen()}));
    let program = Value::Array(program);
    assert!(program.as_array().unwrap().iter().all(effect_supported));
    let mut provider = LastChoice::default();
    engine
        .execute_effects(
            &program,
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("the lowered program executes");
    let attached_to = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == sword)
        .and_then(|card| card.attached_to.clone());
    assert_eq!(attached_to.as_deref(), Some(bear.as_str()));
}
