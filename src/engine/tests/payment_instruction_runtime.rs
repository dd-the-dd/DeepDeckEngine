//! Payments read at resolution (CR 118.3, 119.4, 107.14, 608.2c, 608.2d):
//! the payment instruction `[PLAYER] pay(s) RESOURCE` (`payCost`, paid when
//! the payer can), its chosen amount restated by `that much`, an offer of it
//! to a population, and a resolution cost whose parts are a coordinated list
//! (`pay {1} and 1 life`). The Oracle text is compiled, then its primitive IR
//! is executed by the ordinary evaluators. Helpers are local so the module
//! also runs on its own.
use super::*;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    let creature = type_line.contains("Creature");
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: id.to_string(),
        power: creature.then(|| "2".to_string()),
        rules: Vec::new(),
        toughness: creature.then(|| "2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn instance(id: &str, player: usize, definition: CardDefinition) -> CardInstance {
    let mut card = card_instance(&format!("player-{player}"), &definition, 0);
    card.instance_id = id.to_string();
    card.summoning_sick = false;
    card
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
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
        29,
    )
    .expect("test setup is valid");
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    for player in &mut engine.state.players {
        player.battlefield.clear();
        player.graveyard.clear();
    }
    engine
}

fn plains(id: &str, player: usize) -> CardInstance {
    let mut land = definition(id, "Basic Land - Plains");
    land.rules = vec![json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "addMana",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "mana": "{W}",
        }],
    })];
    instance(id, player, land)
}

fn admit(engine: &mut GameEngine) {
    for index in 0..engine.state.players.len() {
        let mut cards = std::mem::take(&mut engine.state.players[index].battlefield);
        for card in &mut cards {
            if GameEngine::tracked_zone_identity(card).is_none() {
                engine
                    .assign_new_object_incarnation(card)
                    .expect("admit test object");
            }
        }
        engine.state.players[index].battlefield = cards;
    }
}

/// The last canonical, executable rule the parser emits for `text`.
fn compile(type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Payment Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("payment-instruction-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    let rule = parsed
        .abilities
        .last()
        .and_then(|ability| ability.rule.clone())
        .expect("canonical rule");
    assert!(rule_is_executable(&rule), "{text}: {rule:#}");
    rule
}

fn kinds(effects: &Value) -> Vec<String> {
    effects
        .as_array()
        .expect("an effect list")
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Accepts (the last option of every resolution choice: perform, pay) for
/// the players in `accepting`, declines for every other player, and picks
/// `amount` (clamped to the offered bounds) for every chosen number.
struct Chooser {
    accepting: Vec<&'static str>,
    amount: i32,
}

impl DecisionProvider for Chooser {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(if self.accepting.contains(&request.player_id.as_str()) {
            request.options.len().saturating_sub(1)
        } else {
            0
        })
    }

    fn choose_number(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let (minimum, maximum) = decision_number_bounds(request)
            .ok_or_else(|| EngineError::new("a number selection was expected"))?;
        Ok(self.amount.clamp(minimum, maximum))
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, provider: &mut Chooser) {
    let stack_object = StackObject {
        id: "stack:payment-instruction".to_string(),
        controller: "player-0".to_string(),
        card: instance("payment-source", 0, definition("payment-source", "Sorcery")),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    admit(engine);
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut stack_object.decisions.clone(),
            provider,
        )
        .expect("the payment program resolves");
}

fn energy(engine: &GameEngine, player: usize) -> i32 {
    engine.state.players[player]
        .counters
        .get("energy")
        .copied()
        .unwrap_or(0)
}

// An unused combination of the Harnessed Lightning shape: the chosen energy
// amount is restated by a life gain instead of damage.
const CHOSEN_ENERGY: &str =
    "You get {E}{E}{E}, then you may pay any amount of {E}. You gain that much life.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn energy_paid_this_way_is_the_quantity_a_later_action_restates() {
    let rule = compile("Sorcery", CHOSEN_ENERGY);
    // `you may pay any amount`: the amount is the decision; no permission
    // wraps it, so the chosen number stays a top-level quantity.
    assert_eq!(
        kinds(&rule["effects"]),
        ["addPlayerCounters", "chooseNumber", "payCost", "gainLife"],
        "{rule:#}"
    );
    let mut engine = engine();
    let life = engine.state.players[0].life;
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 2,
        },
    );
    assert_eq!(energy(&engine, 0), 1, "two of the three energy were paid");
    assert_eq!(engine.state.players[0].life, life + 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_chosen_amount_never_exceeds_what_the_payer_has() {
    let rule = compile("Sorcery", CHOSEN_ENERGY);
    let mut engine = engine();
    let life = engine.state.players[0].life;
    // The payer asks for more than they have: the bound is the energy held.
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 9,
        },
    );
    assert_eq!(energy(&engine, 0), 0);
    assert_eq!(engine.state.players[0].life, life + 3);
    // Zero is one of the amounts: nothing is paid and nothing restated.
    let mut engine = self::engine();
    let life = engine.state.players[0].life;
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 0,
        },
    );
    assert_eq!(energy(&engine, 0), 3);
    assert_eq!(engine.state.players[0].life, life);
}

// Rhystic Circle's offer of a payment, with a life payment and a draw.
const OFFERED_LIFE: &str = "Any opponent may pay 4 life. If no one does, draw two cards.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_offered_payment_runs_the_declined_outcome_only_when_nobody_pays() {
    let rule = compile("Sorcery", OFFERED_LIFE);
    assert_eq!(rule["effects"][0]["kind"], "offerAction", "{rule:#}");
    assert_eq!(rule["effects"][0]["action"]["kind"], "payCost");

    // Nobody accepts: the controller draws two cards.
    let mut engine = engine();
    let hand = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: Vec::new(),
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[0].hand.len(), hand + 2);
    assert_eq!(engine.state.players[1].life, 20);

    // The opponent pays: four life leaves them and nobody draws.
    let mut engine = self::engine();
    let hand = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-1"],
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[1].life, 16);
    assert_eq!(engine.state.players[0].hand.len(), hand);

    // An opponent who can't pay accepts in vain (CR 119.4): not performed.
    let mut engine = self::engine();
    engine.state.players[1].life = 3;
    let hand = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-1"],
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[1].life, 3);
    assert_eq!(engine.state.players[0].hand.len(), hand + 2);
}

// Miara's coordinated cost: one mana part and one gapped life part.
const MANA_AND_LIFE: &str =
    "Whenever this creature attacks, you may pay {1} and 1 life. If you do, draw a card.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_coordinated_mana_and_life_cost_is_paid_whole_or_not_at_all() {
    let rule = compile("Creature — Elf Scout", MANA_AND_LIFE);
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "optionalPayCostPerformEffects", "{rule:#}");
    assert_eq!(
        effect["cost"]["costs"],
        json!([
            { "kind": "payMana", "manaCost": "{1}" },
            { "kind": "payLife", "amount": { "kind": "integer", "value": 1 } },
        ])
    );

    // Both parts are payable: one land is tapped and one life is paid.
    let mut engine = engine();
    engine.state.players[0].battlefield = vec![plains("plains", 0)];
    let hand = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[0].life, 19);
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);

    // No mana: the whole cost is unpayable, so no life is paid either.
    let mut engine = self::engine();
    let hand = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[0].life, 20);
    assert_eq!(engine.state.players[0].hand.len(), hand);
}

// A scaled life payment: an amount that is an expression, not a literal.
const SCALED_LIFE: &str = "Draw a card, then pay life equal to the number of cards in your hand.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_scaled_life_payment_reads_its_amount_at_resolution() {
    let rule = compile("Sorcery", SCALED_LIFE);
    let payment = rule["effects"]
        .as_array()
        .and_then(|effects| effects.iter().find(|effect| effect["kind"] == "payCost"))
        .expect("a payment instruction");
    assert_eq!(payment["cost"]["kind"], "payLife", "{rule:#}");
    let mut engine = engine();
    let hand = engine.state.players[0].hand.len() as i32;
    resolve(
        &mut engine,
        &rule,
        &mut Chooser {
            accepting: vec!["player-0"],
            amount: 0,
        },
    );
    assert_eq!(engine.state.players[0].life, 20 - (hand + 1));
}
