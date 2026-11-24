use super::*;

// Non-controller performers (CR 608.2c, 608.2d): an instruction offered to
// any player until one performs it, an action performed once per affected
// object by that object's player, and mana another player adds. Real Oracle
// texts, parsed and lowered by the production compiler.

const DESECRATION_DEMON: &str = "Flying\nAt the beginning of each combat, any opponent may sacrifice a creature of their choice. If a player does, tap this creature and put a +1/+1 counter on it.";
const TITHE: &str = "At the beginning of your upkeep, any opponent may discard a card. If no one does, you draw a card.";
const RAMPAGE: &str = "Destroy all artifacts and enchantments. For each permanent destroyed this way, its controller creates a 3/3 green Centaur creature token.";
const MANA_FLARE: &str = "Whenever a player taps a land for mana, that player adds one mana of any type that land produced.";

/// Accepts (or declines) every offer; any other decision takes its first
/// option.
struct Offer(bool);
impl DecisionProvider for Offer {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| {
                option.decisions.get("performOfferedAction") == Some(&Value::Bool(self.0))
            })
            .unwrap_or(0))
    }
}

fn parsed_rule(name: &str, type_line: &str, text: &str, ability: usize) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[ability]
        .rule
        .clone()
        .expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn card(engine: &mut GameEngine, id: &str, type_line: &str, owner: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, type_line), owner);
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

/// Resolves every top-level effect of `rule` in order, as one resolution.
fn resolve_rule(
    engine: &mut GameEngine,
    source: CardInstance,
    rule: &Value,
    decisions: BTreeMap<String, Value>,
    provider: &mut impl DecisionProvider,
) {
    let stack_object = StackObject {
        id: "stack:performer".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(rule["kind"].as_str().unwrap_or("").to_string()),
        ability_rule: Some(rule.clone()),
        decisions: decisions.clone(),
        targets: BTreeMap::new(),
    };
    let mut bindings = BTreeMap::new();
    let mut resolution_decisions = decisions;
    for effect in rule["effects"].as_array().expect("effects") {
        engine
            .execute_effect(
                effect,
                None,
                &stack_object,
                &mut bindings,
                &mut resolution_decisions,
                provider,
            )
            .expect("the program resolves");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_offered_sacrifice_runs_as_the_taker_and_gates_its_outcome() {
    let rule = parsed_rule(
        "Desecration Demon",
        "Creature — Demon",
        DESECRATION_DEMON,
        1,
    );
    assert_eq!(rule["effects"][0]["kind"], "offerAction", "{rule:#}");

    // The opponent takes the offer: its own creature is sacrificed, and the
    // outcome taps the demon (CR 608.2c).
    let mut engine = test_engine(2);
    let demon = card(&mut engine, "demon", "Creature — Demon", "player-0");
    let victim = card(&mut engine, "victim", "Creature — Bear", "player-1");
    engine.state.players[0].battlefield = vec![demon.clone()];
    engine.state.players[1].battlefield = vec![victim];
    resolve_rule(
        &mut engine,
        demon.clone(),
        &rule,
        BTreeMap::new(),
        &mut Offer(true),
    );
    assert!(
        engine.state.players[1].battlefield.is_empty(),
        "the taker sacrificed"
    );
    assert!(
        engine.state.players[0].battlefield[0].tapped,
        "the outcome resolved"
    );

    // Declined: nothing is sacrificed and the demon stays untapped.
    let mut engine = test_engine(2);
    let victim = card(&mut engine, "victim", "Creature — Bear", "player-1");
    engine.state.players[0].battlefield = vec![demon.clone()];
    engine.state.players[1].battlefield = vec![victim];
    resolve_rule(
        &mut engine,
        demon.clone(),
        &rule,
        BTreeMap::new(),
        &mut Offer(false),
    );
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
    assert!(!engine.state.players[0].battlefield[0].tapped);

    // An opponent without a creature can't take it (CR 608.2d).
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![demon.clone()];
    resolve_rule(&mut engine, demon, &rule, BTreeMap::new(), &mut Offer(true));
    assert!(!engine.state.players[0].battlefield[0].tapped);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn nobody_taking_an_offer_runs_the_declined_outcome() {
    let rule = parsed_rule("Synthetic Tithe", "Enchantment", TITHE, 0);
    let mut engine = test_engine(2);
    let source = card(&mut engine, "tithe", "Enchantment", "player-0");
    let drawn = card(&mut engine, "drawn", "Sorcery", "player-0");
    let held = card(&mut engine, "held", "Instant", "player-1");
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[0].library = vec![drawn];
    engine.state.players[1].hand = vec![held];
    resolve_rule(
        &mut engine,
        source.clone(),
        &rule,
        BTreeMap::new(),
        &mut Offer(false),
    );
    assert_eq!(engine.state.players[0].hand.len(), 1, "no one did: draw");
    assert_eq!(engine.state.players[1].hand.len(), 1);

    // Taken: the opponent discards and the controller draws nothing.
    let mut engine = test_engine(2);
    let drawn = card(&mut engine, "drawn", "Sorcery", "player-0");
    let held = card(&mut engine, "held", "Instant", "player-1");
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[0].library = vec![drawn];
    engine.state.players[1].hand = vec![held];
    resolve_rule(
        &mut engine,
        source,
        &rule,
        BTreeMap::new(),
        &mut Offer(true),
    );
    assert!(
        engine.state.players[1].hand.is_empty(),
        "the taker discarded"
    );
    assert!(
        engine.state.players[0].hand.is_empty(),
        "a player did: no draw"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn each_destroyed_permanent_gives_its_own_controller_a_token() {
    let rule = parsed_rule("Rampage of the Clans", "Instant", RAMPAGE, 0);
    let mut engine = test_engine(2);
    let source = card(&mut engine, "rampage", "Instant", "player-0");
    let artifact = card(&mut engine, "artifact", "Artifact", "player-0");
    let enchantment = card(&mut engine, "enchantment", "Enchantment", "player-1");
    let other = card(&mut engine, "other", "Enchantment", "player-1");
    engine.state.players[0].battlefield = vec![artifact];
    engine.state.players[1].battlefield = vec![enchantment, other];
    resolve_rule(
        &mut engine,
        source,
        &rule,
        BTreeMap::new(),
        &mut Offer(true),
    );
    let tokens = |player: usize| {
        engine.state.players[player]
            .battlefield
            .iter()
            .filter(|permanent| permanent.definition.is_token)
            .count()
    };
    // CR 608.2h: each token goes to the destroyed permanent's controller.
    assert_eq!(tokens(0), 1);
    assert_eq!(tokens(1), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn another_player_adds_a_type_the_tapped_land_produced() {
    let rule = parsed_rule("Mana Flare", "Enchantment", MANA_FLARE, 0);
    let mut engine = test_engine(2);
    let source = card(&mut engine, "flare", "Enchantment", "player-0");
    engine.state.players[0].battlefield = vec![source.clone()];
    let decisions = BTreeMap::from([
        ("triggeringPlayerId".to_string(), json!("player-1")),
        ("manaProduced".to_string(), json!(["G"])),
    ]);
    resolve_rule(
        &mut engine,
        source.clone(),
        &rule,
        decisions,
        &mut Offer(true),
    );
    assert!(
        engine.state.players[1]
            .mana_pool
            .iter()
            .any(|mana| mana.symbol == "G"),
        "the tapping player adds the produced type"
    );
    assert!(engine.state.players[0].mana_pool.is_empty());

    // A record that produced nothing adds nothing, never a guessed type.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];
    let decisions = BTreeMap::from([
        ("triggeringPlayerId".to_string(), json!("player-1")),
        ("manaProduced".to_string(), json!([])),
    ]);
    resolve_rule(&mut engine, source, &rule, decisions, &mut Offer(true));
    assert!(engine.state.players[1].mana_pool.is_empty());
}
