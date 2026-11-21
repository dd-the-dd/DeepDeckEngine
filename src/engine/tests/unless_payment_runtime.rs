//! `EFFECT unless PAYER COST` (CR 118.12): the parser lowers the frame to one
//! resolution cost attempt by the named payer followed by the guarded actions
//! when it was not paid. The payer, not the ability controller, makes the
//! payment choice; an unpayable cost counts as not paid. `unless CONDITION`
//! guards the actions with the condition's negation at resolution.
//! Helpers are local so the module also runs on its own.
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

/// The first canonical, executable rule the parser emits for `text`.
fn parsed_rule(type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Unless Source".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
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

/// Pays (or declines) every offered resolution cost and records who chose.
struct Payer {
    pay: bool,
    choosers: Vec<String>,
}

impl Payer {
    fn new(pay: bool) -> Self {
        Self {
            pay,
            choosers: Vec::new(),
        }
    }
}

impl DecisionProvider for Payer {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let wanted = if self.pay {
            ActionKind::ChooseResolution
        } else {
            ActionKind::DeclinePayment
        };
        if request.options.iter().any(|option| {
            matches!(
                option.kind,
                ActionKind::ChooseResolution | ActionKind::DeclinePayment
            )
        }) {
            self.choosers.push(request.player_id.clone());
        }
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == wanted)
            .unwrap_or(0))
    }
}

/// Resolves every effect of `rule` for an object controlled by player 0.
fn resolve(
    engine: &mut GameEngine,
    rule: &Value,
    source: CardInstance,
    targets: &[(&str, TargetRef)],
    provider: &mut impl DecisionProvider,
) {
    admit(engine);
    // A permanent source resolves with its admitted battlefield identity.
    let source = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == source.instance_id)
        .cloned()
        .unwrap_or(source);
    let stack_object = StackObject {
        id: "stack:unless".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(rule["kind"].as_str().unwrap_or_default().to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: targets
            .iter()
            .map(|(id, target)| (id.to_string(), target.clone()))
            .collect(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the unless frame resolves");
}

fn on_battlefield(engine: &GameEngine, player: usize, id: &str) -> bool {
    engine.state.players[player]
        .battlefield
        .iter()
        .any(|card| card.instance_id == id)
}

#[test]
fn the_controller_pays_or_the_guarded_sacrifice_happens() {
    // Archway Commons pattern: CR 118.12 with the ability controller paying.
    let rule = parsed_rule(
        "Land",
        "When this land enters, sacrifice it unless you pay {1}.",
    );
    assert_eq!(rule["effects"][0]["kind"], "tryPayCost");
    let setup = |mana: bool| {
        let mut engine = self::engine();
        let land = instance("source", 0, definition("source", "Land"));
        engine.state.players[0].battlefield = vec![land];
        if mana {
            engine.state.players[0]
                .battlefield
                .push(plains("plains", 0));
        }
        engine
    };

    let mut engine = setup(true);
    let source = engine.state.players[0].battlefield[0].clone();
    let mut payer = Payer::new(true);
    resolve(&mut engine, &rule, source, &[], &mut payer);
    assert!(on_battlefield(&engine, 0, "source"), "paid: kept");
    assert_eq!(payer.choosers, ["player-0"]);

    let mut engine = setup(true);
    let source = engine.state.players[0].battlefield[0].clone();
    resolve(&mut engine, &rule, source, &[], &mut Payer::new(false));
    assert!(
        !on_battlefield(&engine, 0, "source"),
        "declined: sacrificed"
    );

    // No mana: the cost cannot be paid, so the sacrifice happens.
    let mut engine = setup(false);
    let source = engine.state.players[0].battlefield[0].clone();
    resolve(&mut engine, &rule, source, &[], &mut Payer::new(true));
    assert!(
        !on_battlefield(&engine, 0, "source"),
        "unpayable: sacrificed"
    );
}

#[test]
fn the_target_controller_chooses_a_life_or_sacrifice_payment() {
    for (text, payable) in [
        (
            "Destroy target artifact unless its controller pays 3 life.",
            true,
        ),
        // A new combination: a conjugated sacrifice paid by the target's controller.
        (
            "Destroy target artifact unless its controller sacrifices a creature of their choice.",
            true,
        ),
        (
            "Destroy target artifact unless its controller sacrifices a creature of their choice.",
            false,
        ),
    ] {
        let rule = parsed_rule("Sorcery", text);
        assert_eq!(
            rule["effects"][0]["player"]["kind"], "controllerOf",
            "{text}"
        );
        for pay in [true, false] {
            let mut engine = self::engine();
            engine.state.players[1].battlefield =
                vec![instance("relic", 1, definition("relic", "Artifact"))];
            if payable {
                engine.state.players[1].battlefield.push(instance(
                    "bear",
                    1,
                    definition("bear", "Creature - Bear"),
                ));
            }
            let spell = instance("spell", 0, definition("spell", "Sorcery"));
            let life = engine.state.players[1].life;
            let mut payer = Payer::new(pay);
            resolve(
                &mut engine,
                &rule,
                spell,
                &[(
                    "targetPermanent",
                    TargetRef::Permanent {
                        instance_id: "relic".to_string(),
                    },
                )],
                &mut payer,
            );
            let paid = pay && payable;
            assert_eq!(on_battlefield(&engine, 1, "relic"), paid, "{text} {pay}");
            assert!(
                payer.choosers.iter().all(|player| player == "player-1"),
                "{text}: only the target's controller chooses: {:?}",
                payer.choosers
            );
            if text.contains("life") {
                assert_eq!(
                    engine.state.players[1].life,
                    life - if paid { 3 } else { 0 }
                );
            } else if payable {
                assert_eq!(on_battlefield(&engine, 1, "bear"), !paid, "{text} {pay}");
            }
        }
    }
}

#[test]
fn an_unless_condition_guards_the_action_with_its_negation() {
    let rule = parsed_rule(
        "Creature - Pirate",
        "When this creature enters, you lose 2 life unless you control another Pirate.",
    );
    for (other_pirate, loss) in [(false, 2), (true, 0)] {
        let mut engine = self::engine();
        let source = instance("source", 0, definition("source", "Creature - Pirate"));
        engine.state.players[0].battlefield = vec![source.clone()];
        if other_pirate {
            engine.state.players[0].battlefield.push(instance(
                "mate",
                0,
                definition("mate", "Creature - Pirate"),
            ));
        }
        let life = engine.state.players[0].life;
        resolve(&mut engine, &rule, source, &[], &mut Payer::new(true));
        assert_eq!(engine.state.players[0].life, life - loss, "{other_pirate}");
    }
}
