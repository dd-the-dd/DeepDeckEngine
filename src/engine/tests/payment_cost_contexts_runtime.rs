//! Cost unification step 2: the parser-emitted payment actions are paid by the
//! engine in each cost context. `another` excludes the source from declared
//! choices, counters are removed from the source or distributed over holders,
//! chosen cards are exiled from a zone, and a resolution cost list is paid
//! all-or-nothing (CR 118.3, 601.2h, 602.2b). Helpers are local so the module
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

fn instance(id: &str, definition: CardDefinition) -> CardInstance {
    let mut card = card_instance("player-0", &definition, 0);
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
        73,
    )
    .expect("test setup is valid");
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].graveyard.clear();
    engine
}

fn plains(id: &str) -> CardInstance {
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
    instance(id, land)
}

/// The first canonical rule the parser emits for `text`.
fn parsed_rule(type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Payment Source".to_string(),
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

fn source(type_line: &str, text: &str) -> CardInstance {
    let mut card = definition("source", type_line);
    card.rules = vec![parsed_rule(type_line, text)];
    instance("source", card)
}

/// Admits every test object not yet given a zone identity.
fn admit(engine: &mut GameEngine) {
    for index in 0..engine.state.players.len() {
        for zone in ["battlefield", "graveyard"] {
            let mut cards = std::mem::take(match zone {
                "battlefield" => &mut engine.state.players[index].battlefield,
                _ => &mut engine.state.players[index].graveyard,
            });
            for card in &mut cards {
                if GameEngine::tracked_zone_identity(card).is_none() {
                    engine
                        .assign_new_object_incarnation(card)
                        .expect("admit test object");
                }
            }
            match zone {
                "battlefield" => engine.state.players[index].battlefield = cards,
                _ => engine.state.players[index].graveyard = cards,
            }
        }
    }
}

fn activations(engine: &mut GameEngine) -> Vec<LegalAction> {
    admit(engine);
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("source")
        })
        .collect()
}

fn activate(engine: &mut GameEngine, action: &LegalAction) {
    engine
        .apply_priority_action(action, &mut RandomAi::seeded(7))
        .expect("the activation pays its costs");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the ability resolves");
}

fn chosen_ids(action: &LegalAction) -> Vec<String> {
    action
        .targets
        .values()
        .filter_map(|target| match target {
            TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                Some(instance_id.clone())
            }
            _ => None,
        })
        .collect()
}

fn counter(engine: &GameEngine, id: &str, kind: &str) -> i32 {
    engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == id)
        .and_then(|card| card.counters.get(kind).copied())
        .unwrap_or(0)
}

#[test]
fn another_excludes_the_source_from_declared_sacrifice_and_tap_choices() {
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![source(
        "Artifact Creature - Construct",
        "Sacrifice another artifact: Draw a card.",
    )];
    assert!(
        activations(&mut engine).is_empty(),
        "the source alone cannot pay `another`"
    );
    engine.state.players[0]
        .battlefield
        .push(instance("fodder", definition("fodder", "Artifact")));
    let actions = activations(&mut engine);
    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| chosen_ids(action) == ["fodder".to_string()])
    );
    let hand = engine.state.players[0].hand.len();
    activate(&mut engine, &actions[0]);
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "fodder")
    );
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);

    // A multi-object choice keys `id:index`; none of them may be the source.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![
        source(
            "Creature - Merfolk",
            "Tap two other untapped creatures you control: Draw a card.",
        ),
        instance("first", definition("first", "Creature - Fish")),
    ];
    assert!(activations(&mut engine).is_empty());
    engine.state.players[0]
        .battlefield
        .push(instance("second", definition("second", "Creature - Fish")));
    let actions = activations(&mut engine);
    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| !chosen_ids(action).contains(&"source".to_string()))
    );
    activate(&mut engine, &actions[0]);
    let tapped = |id: &str| {
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|card| card.instance_id == id)
            .is_some_and(|card| card.tapped)
    };
    assert!(tapped("first") && tapped("second") && !tapped("source"));
}

fn in_hand(engine: &GameEngine, id: &str) -> bool {
    engine.state.players[0]
        .hand
        .iter()
        .any(|card| card.instance_id == id)
}

#[test]
fn return_costs_bounce_a_chosen_permanent_or_the_source() {
    // Quirion Ranger shape: one chosen land the payer controls.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![source(
        "Creature - Elf",
        "Return a land you control to its owner's hand: Draw a card.",
    )];
    assert!(activations(&mut engine).is_empty(), "no land to return");
    engine.state.players[0].battlefield.push(plains("land"));
    let actions = activations(&mut engine);
    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| chosen_ids(action) == ["land".to_string()])
    );
    let hand = engine.state.players[0].hand.len();
    activate(&mut engine, &actions[0]);
    assert!(in_hand(&engine, "land"));
    assert_eq!(
        engine.state.players[0].hand.len(),
        hand + 2,
        "land and the drawn card"
    );

    // Two chosen lands: one declaration of two distinct permanents.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![
        source(
            "Creature - Moonfolk",
            "Return two lands you control to their owner's hand: Draw a card.",
        ),
        plains("first"),
    ];
    assert!(
        activations(&mut engine).is_empty(),
        "one land cannot pay two"
    );
    engine.state.players[0].battlefield.push(plains("second"));
    let actions = activations(&mut engine);
    assert!(!actions.is_empty());
    activate(&mut engine, &actions[0]);
    assert!(in_hand(&engine, "first") && in_hand(&engine, "second"));

    // Attunement shape: the source itself returns while its ability resolves.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![source(
        "Enchantment",
        "Return this enchantment to its owner's hand: Draw a card.",
    )];
    let hand = engine.state.players[0].hand.len();
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    activate(&mut engine, &actions[0]);
    assert!(in_hand(&engine, "source"));
    assert_eq!(engine.state.players[0].hand.len(), hand + 2);
}

#[test]
fn put_counter_costs_mark_the_source_or_a_chosen_permanent() {
    // Devoted Druid shape: counters on the source.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![source(
        "Creature - Elf Druid",
        "Put a -1/-1 counter on this creature: Draw a card.",
    )];
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    activate(&mut engine, &actions[0]);
    assert_eq!(counter(&engine, "source", "-1/-1"), 1);

    // Hatchet Bully shape: a chosen creature the payer controls.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![
        source(
            "Artifact",
            "Put a +1/+1 counter on a creature you control: Draw a card.",
        ),
        instance("bear", definition("bear", "Creature - Bear")),
    ];
    let actions = activations(&mut engine);
    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| chosen_ids(action) == ["bear".to_string()])
    );
    activate(&mut engine, &actions[0]);
    assert_eq!(counter(&engine, "bear", "+1/+1"), 1);
}

#[test]
fn source_counters_are_removed_by_kind_or_by_declared_choice() {
    // Named kind and quantity.
    let mut engine = self::engine();
    let mut artifact = source(
        "Artifact",
        "Remove two charge counters from this artifact: Draw a card.",
    );
    artifact.counters.insert("charge".to_string(), 1);
    engine.state.players[0].battlefield = vec![artifact];
    assert!(
        activations(&mut engine).is_empty(),
        "one counter cannot pay two"
    );
    engine.state.players[0].battlefield[0]
        .counters
        .insert("charge".to_string(), 3);
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    activate(&mut engine, &actions[0]);
    assert_eq!(counter(&engine, "source", "charge"), 1);

    // One unnamed counter: its kind is a declared choice.
    let mut engine = self::engine();
    let mut creature = source(
        "Creature - Faerie",
        "Remove a counter from this creature: Draw a card.",
    );
    creature.counters.insert("+1/+1".to_string(), 1);
    creature.counters.insert("stun".to_string(), 1);
    engine.state.players[0].battlefield = vec![creature];
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 2, "one declaration per available kind");
    let stun = actions
        .iter()
        .find(|action| action.decisions.values().any(|value| value == "stun"))
        .expect("the stun counter can pay");
    activate(&mut engine, stun);
    assert_eq!(counter(&engine, "source", "stun"), 0);
    assert_eq!(counter(&engine, "source", "+1/+1"), 1);
}

#[test]
fn unnamed_and_controlled_counters_are_one_distribution_while_paying() {
    // Two unnamed counters on the source may be of two kinds.
    let mut engine = self::engine();
    let mut creature = source(
        "Creature - Treefolk",
        "Remove two counters from this creature: Draw a card.",
    );
    creature.counters.insert("+1/+1".to_string(), 1);
    engine.state.players[0].battlefield = vec![creature];
    assert!(activations(&mut engine).is_empty());
    engine.state.players[0].battlefield[0]
        .counters
        .insert("-1/-1".to_string(), 1);
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1, "one distinct distribution");
    activate(&mut engine, &actions[0]);
    assert_eq!(counter(&engine, "source", "+1/+1"), 0);
    assert_eq!(counter(&engine, "source", "-1/-1"), 0);

    // A named counter from a creature the payer controls, never an opponent's.
    let mut engine = self::engine();
    let mut holder = instance("holder", definition("holder", "Creature - Turtle"));
    holder.counters.insert("+1/+1".to_string(), 2);
    let mut foreign = instance("foreign", definition("foreign", "Creature - Rat"));
    foreign.controller = "player-1".to_string();
    foreign.owner = "player-1".to_string();
    foreign.counters.insert("+1/+1".to_string(), 5);
    engine.state.players[0].battlefield = vec![
        source(
            "Artifact",
            "Remove a +1/+1 counter from a creature you control: Draw a card.",
        ),
        holder,
    ];
    engine.state.players[1].battlefield = vec![foreign];
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    activate(&mut engine, &actions[0]);
    assert_eq!(counter(&engine, "holder", "+1/+1"), 1);
    assert_eq!(engine.state.players[1].battlefield[0].counters["+1/+1"], 5);
}

#[test]
fn chosen_cards_are_exiled_from_the_graveyard_and_another_excludes_the_source() {
    // Battlefield source, two chosen cards.
    let mut engine = self::engine();
    engine.state.players[0].battlefield = vec![source(
        "Artifact",
        "Exile two cards from your graveyard: Draw a card.",
    )];
    engine.state.players[0].graveyard = vec![instance("a", definition("a", "Instant"))];
    assert!(activations(&mut engine).is_empty());
    engine.state.players[0]
        .graveyard
        .push(instance("b", definition("b", "Sorcery")));
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    activate(&mut engine, &actions[0]);
    assert!(engine.state.players[0].graveyard.is_empty());
    assert_eq!(engine.state.players[0].exile.len(), 2);

    // Graveyard source: `another creature card` never chooses the source.
    let mut engine = self::engine();
    let card = source(
        "Creature - Spirit",
        "Exile another creature card from your graveyard, Exile this card from your graveyard: Draw a card.",
    );
    assert_eq!(card.definition.rules[0]["activationZone"], "graveyard");
    engine.state.players[0].graveyard = vec![card];
    assert!(
        activations(&mut engine).is_empty(),
        "the source cannot pay `another`"
    );
    engine.state.players[0].graveyard.push(instance(
        "spirit",
        definition("spirit", "Creature - Spirit"),
    ));
    let actions = activations(&mut engine);
    assert_eq!(actions.len(), 1);
    assert_eq!(chosen_ids(&actions[0]), ["spirit".to_string()]);
    let hand = engine.state.players[0].hand.len();
    activate(&mut engine, &actions[0]);
    let exiled = engine.state.players[0]
        .exile
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<Vec<_>>();
    assert!(exiled.contains(&"spirit") && exiled.contains(&"source"));
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
}

/// Pays any offered resolution cost.
struct Pay;
impl DecisionProvider for Pay {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == ActionKind::ChooseResolution)
            .unwrap_or(0))
    }
}

/// Declines every resolution cost.
struct Decline;
impl DecisionProvider for Decline {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == ActionKind::DeclinePayment)
            .unwrap_or(0))
    }
}

fn resolve_effect(engine: &mut GameEngine, rule: &Value, provider: &mut impl DecisionProvider) {
    admit(engine);
    let source = engine.state.players[0].battlefield[0].clone();
    let stack_object = StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effect(
            &rule["effects"][0],
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the optional cost program resolves");
}

#[test]
fn a_resolution_cost_list_is_paid_entirely_or_not_at_all() {
    let rule = parsed_rule(
        "Creature - Human",
        "At the beginning of combat on your turn, you may pay {1} and exile a card from your graveyard. If you do, draw a card.",
    );
    assert_eq!(rule["effects"][0]["cost"]["kind"], "costSequence");
    let setup = |land: bool, card: bool| {
        let mut engine = self::engine();
        let mut battlefield = vec![instance("source", definition("source", "Creature - Human"))];
        if land {
            battlefield.push(plains("plains"));
        }
        engine.state.players[0].battlefield = battlefield;
        if card {
            engine.state.players[0].graveyard = vec![instance("old", definition("old", "Instant"))];
        }
        engine
    };
    let land_tapped = |engine: &GameEngine| {
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "plains" && card.tapped)
    };

    // CR 118.3: one unpayable part makes the whole list unpayable.
    for (land, card) in [(true, false), (false, true)] {
        let mut engine = setup(land, card);
        let hand = engine.state.players[0].hand.len();
        resolve_effect(&mut engine, &rule, &mut Pay);
        assert_eq!(engine.state.players[0].hand.len(), hand, "{land} {card}");
        assert!(!land_tapped(&engine), "no mana is spent: {land} {card}");
        assert_eq!(engine.state.players[0].graveyard.len(), usize::from(card));
    }

    let mut engine = setup(true, true);
    let hand = engine.state.players[0].hand.len();
    resolve_effect(&mut engine, &rule, &mut Decline);
    assert_eq!(engine.state.players[0].hand.len(), hand);
    assert!(!land_tapped(&engine));
    assert_eq!(engine.state.players[0].graveyard.len(), 1);

    resolve_effect(&mut engine, &rule, &mut Pay);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
    assert!(land_tapped(&engine));
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "old")
    );
}

#[test]
fn resolution_counter_removal_and_card_exile_pay_their_own_resources() {
    let rule = parsed_rule(
        "Creature - Faerie",
        "At the beginning of your upkeep, you may remove a counter from this creature. If you do, draw a card.",
    );
    let mut engine = self::engine();
    let mut creature = instance("source", definition("source", "Creature - Faerie"));
    creature.counters.insert("stun".to_string(), 1);
    engine.state.players[0].battlefield = vec![creature];
    let hand = engine.state.players[0].hand.len();
    resolve_effect(&mut engine, &rule, &mut Pay);
    assert_eq!(counter(&engine, "source", "stun"), 0);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
    // No counter left: the cost is unpayable.
    resolve_effect(&mut engine, &rule, &mut Pay);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);

    let rule = parsed_rule(
        "Creature - Human",
        "At the beginning of your upkeep, you may exile a creature card from your graveyard. If you do, draw a card.",
    );
    let mut engine = self::engine();
    engine.state.players[0].battlefield =
        vec![instance("source", definition("source", "Creature - Human"))];
    engine.state.players[0].graveyard = vec![instance("spell", definition("spell", "Sorcery"))];
    let hand = engine.state.players[0].hand.len();
    resolve_effect(&mut engine, &rule, &mut Pay);
    assert_eq!(engine.state.players[0].hand.len(), hand, "no creature card");
    engine.state.players[0]
        .graveyard
        .push(instance("beast", definition("beast", "Creature - Beast")));
    resolve_effect(&mut engine, &rule, &mut Pay);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert_eq!(engine.state.players[0].exile[0].instance_id, "beast");
}
