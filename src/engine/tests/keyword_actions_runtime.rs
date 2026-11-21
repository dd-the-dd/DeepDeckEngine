//! Runtime proofs for keyword actions (CR 701, 2026-11-06) read as ordinary
//! verbs: each real Oracle text is parsed, and the primitive program it
//! lowers to is executed. No keyword-action opcode reaches the engine.
use super::*;

fn parsed_rules(type_line: &str, text: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Keyword Action Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("keyword-actions-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect::<Vec<_>>();
    assert!(rules.iter().all(rule_is_executable), "{rules:#?}");
    rules
}

fn rule_kinds(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                found.push(kind.to_string());
            }
            object.values().for_each(|value| rule_kinds(value, found));
        }
        Value::Array(values) => values.iter().for_each(|value| rule_kinds(value, found)),
        _ => {}
    }
}

fn stack_object(source: &CardInstance, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:keyword-action".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets,
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the keyword action resolves");
}

fn creature(id: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, "Creature - Human"), owner)
}

fn on_battlefield<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("the permanent is on the battlefield")
}

fn plus_one(engine: &GameEngine, id: &str) -> i32 {
    on_battlefield(engine, id)
        .counters
        .get("+1/+1")
        .copied()
        .unwrap_or(0)
}

/// Explore (CR 701.44a): a land on top goes to hand without a counter; a
/// nonland gives the exploring permanent a +1/+1 counter instead.
#[test]
fn explore_moves_a_revealed_land_or_counters_the_exploring_creature() {
    let rules = parsed_rules(
        "Creature - Merfolk Scout",
        "When this creature enters, it explores. (Reveal the top card of your library. Put that card into your hand if it's a land. Otherwise, put a +1/+1 counter on this creature, then put the card back or put it into your graveyard.)",
    );
    let mut found = Vec::new();
    rule_kinds(&rules[0], &mut found);
    assert!(found.iter().any(|kind| kind == "emitAbilitySignal"));
    assert!(!found.iter().any(|kind| kind == "explore"));

    // The fixture library holds only lands.
    let mut engine = test_engine(2);
    let scout = creature("scout", "player-0");
    engine.state.players[0].battlefield.push(scout.clone());
    let hand_before = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&scout, BTreeMap::new()),
    );
    assert_eq!(engine.state.players[0].hand.len(), hand_before + 1);
    assert_eq!(plus_one(&engine, "scout"), 0);

    let mut engine = test_engine(2);
    let scout = creature("scout", "player-0");
    engine.state.players[0].battlefield.push(scout.clone());
    // The top of a library is the last element of the zone vector.
    engine.state.players[0].library.push(test_instance(
        "top-spell",
        test_definition("top-spell", "Sorcery"),
        "player-0",
    ));
    let hand_before = engine.state.players[0].hand.len();
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&scout, BTreeMap::new()),
    );
    assert_eq!(engine.state.players[0].hand.len(), hand_before);
    assert_eq!(plus_one(&engine, "scout"), 1);
}

/// Connive (CR 701.50a): draw, discard, and a counter only for a nonland
/// discard. The fixture hand holds only lands plus the drawn card.
#[test]
fn connive_counts_only_nonland_discards() {
    let rules = parsed_rules(
        "Creature - Human Rogue",
        "{3}: This creature connives. (Draw a card, then discard a card. If you discarded a nonland card, put a +1/+1 counter on this creature.)",
    );
    let mut engine = test_engine(2);
    let grifter = creature("grifter", "player-0");
    engine.state.players[0].battlefield.push(grifter.clone());
    let hand_before = engine.state.players[0].hand.len();
    let graveyard_before = engine.state.players[0].graveyard.len();
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&grifter, BTreeMap::new()),
    );
    assert_eq!(engine.state.players[0].hand.len(), hand_before);
    assert_eq!(
        engine.state.players[0].graveyard.len(),
        graveyard_before + 1
    );
    let discarded = engine.state.players[0].graveyard.last().expect("a discard");
    let expected = i32::from(!has_card_type(&discarded.definition, "Land"));
    assert_eq!(plus_one(&engine, "grifter"), expected);
}

/// Endure N (CR 701.63a): counters on the permanent or an N/N Spirit, never both.
#[test]
fn endure_puts_counters_or_creates_one_spirit() {
    let rules = parsed_rules(
        "Creature - Dog Soldier",
        "When this creature enters, it endures 1. (Put a +1/+1 counter on it or create a 1/1 white Spirit creature token.)",
    );
    let mut engine = test_engine(2);
    let guard = creature("guard", "player-0");
    engine.state.players[0].battlefield.push(guard.clone());
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&guard, BTreeMap::new()),
    );
    let spirits = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|card| card.definition.is_token && card.definition.name == "Spirit Token")
        .count();
    assert_eq!(plus_one(&engine, "guard") + spirits as i32, 1);
}

/// Adapt N (CR 701.46a) and Monstrosity N (CR 701.37a) apply once.
#[test]
fn adapt_and_monstrosity_apply_only_once() {
    for (text, counters) in [
        (
            "{2}{U}: Adapt 2. (If this creature has no +1/+1 counters on it, put two +1/+1 counters on it.)",
            2,
        ),
        (
            "{6}{G}: Monstrosity 4. (If this creature isn't monstrous, put four +1/+1 counters on it and it becomes monstrous.)",
            4,
        ),
    ] {
        let rules = parsed_rules("Creature - Snake", text);
        let mut engine = test_engine(2);
        let beast = creature("beast", "player-0");
        engine.state.players[0].battlefield.push(beast.clone());
        let stack = stack_object(&beast, BTreeMap::new());
        resolve(&mut engine, &rules[0], &stack);
        assert_eq!(plus_one(&engine, "beast"), counters, "{text}");
        let stack = stack_object(on_battlefield(&engine, "beast"), BTreeMap::new());
        resolve(&mut engine, &rules[0], &stack);
        assert_eq!(plus_one(&engine, "beast"), counters, "{text}");
    }
}

/// Suspect (CR 701.60a) and investigate (CR 701.16a) compose in one
/// sequence that no card was used to design.
#[test]
fn suspect_and_investigate_compose_with_a_declared_target() {
    let rules = parsed_rules(
        "Creature - Human Detective",
        "When this creature dies, suspect target creature an opponent controls and investigate.",
    );
    let mut engine = test_engine(2);
    let source = creature("detective", "player-0");
    let suspect = creature("suspect", "player-1");
    engine.state.players[1].battlefield.push(suspect);
    let decision = rules[0]["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = stack_object(
        &source,
        BTreeMap::from([(
            decision,
            TargetRef::Permanent {
                instance_id: "suspect".to_string(),
            },
        )]),
    );
    resolve(&mut engine, &rules[0], &stack);
    assert_eq!(
        on_battlefield(&engine, "suspect").flags.get("suspected"),
        Some(&true)
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|card| card.definition.is_token && card.definition.name == "Clue")
    );
}

/// Detain (CR 701.35a): can't attack or block and no activations until the
/// detaining player's next turn.
#[test]
fn detain_prohibits_activation_until_the_controllers_next_turn() {
    let rules = parsed_rules(
        "Creature - Human Soldier",
        "When this creature enters, detain target creature an opponent controls. (Until your next turn, that creature can't attack or block and its activated abilities can't be activated.)",
    );
    let mut engine = test_engine(2);
    let source = creature("arrester", "player-0");
    let detained = creature("detained", "player-1");
    engine.state.players[1].battlefield.push(detained);
    let decision = rules[0]["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = stack_object(
        &source,
        BTreeMap::from([(
            decision,
            TargetRef::Permanent {
                instance_id: "detained".to_string(),
            },
        )]),
    );
    resolve(&mut engine, &rules[0], &stack);
    let detained = on_battlefield(&engine, "detained").clone();
    assert!(engine.activation_prohibited(&detained, false));
    assert!(engine.activation_prohibited(&detained, true));
    assert!(engine.permanent_has_keyword(&detained, "cantAttack"));
    assert!(engine.permanent_has_keyword(&detained, "cantBlock"));
}

/// Populate (CR 701.36a-b) copies a creature token, and does nothing without one.
#[test]
fn populate_copies_a_creature_token_you_control() {
    let rules = parsed_rules(
        "Enchantment",
        "At the beginning of your upkeep, populate. (Create a token that's a copy of a creature token you control.)",
    );
    let mut engine = test_engine(2);
    let source = test_instance("ranks", test_definition("ranks", "Enchantment"), "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&source, BTreeMap::new()),
    );
    assert_eq!(engine.state.players[0].battlefield.len(), 1);

    let mut definition = test_definition("Centaur", "Token Creature - Centaur");
    definition.is_token = true;
    engine.state.players[0]
        .battlefield
        .push(test_instance("centaur", definition, "player-0"));
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&source, BTreeMap::new()),
    );
    let centaurs = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|card| card.definition.name == "Centaur")
        .count();
    assert_eq!(centaurs, 2);
}

/// Support N (CR 701.41a) on a permanent excludes that permanent.
#[test]
fn support_from_a_permanent_targets_other_creatures() {
    let rules = parsed_rules(
        "Creature - Kor Knight Ally",
        "When this creature enters, support 3. (Put a +1/+1 counter on each of up to three other target creatures.)",
    );
    let decision = &rules[0]["declaration"]["decisions"][0];
    assert_eq!(decision["candidates"]["excludeSource"], true);
    assert_eq!(decision["maximum"], 3);
    let mut engine = test_engine(2);
    let source = creature("captain", "player-0");
    let ally = creature("ally", "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].battlefield.push(ally);
    let stack = stack_object(
        &source,
        BTreeMap::from([(
            decision["id"].as_str().expect("target id").to_string(),
            TargetRef::Permanent {
                instance_id: "ally".to_string(),
            },
        )]),
    );
    resolve(&mut engine, &rules[0], &stack);
    assert_eq!(plus_one(&engine, "ally"), 1);
    assert_eq!(plus_one(&engine, "captain"), 0);
}

/// Discover N (CR 701.57a): the first nonland card with mana value N or less
/// is cast for free or put into hand; the other exiled cards go to the bottom.
#[test]
fn discover_puts_an_uncast_hit_into_hand_and_bottoms_the_rest() {
    let rules = parsed_rules(
        "Sorcery",
        "Discover 4. (Exile cards from the top of your library until you exile a nonland card with mana value 4 or less. Cast it without paying its mana cost or put it into your hand. Put the rest on the bottom in a random order.)",
    );
    let mut engine = test_engine(2);
    let library = &mut engine.state.players[0].library;
    // Top of library is the last element: a land is exiled first, then the hit.
    library.push(test_instance(
        "hit",
        test_definition("hit", "Sorcery"),
        "player-0",
    ));
    library.push(test_instance(
        "miss",
        test_definition("miss", "Land"),
        "player-0",
    ));
    let library_size = library.len();
    let source = test_instance(
        "discovery",
        test_definition("discovery", "Sorcery"),
        "player-0",
    );
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&source, BTreeMap::new()),
    );
    let player = &engine.state.players[0];
    assert!(player.exile.is_empty());
    assert!(player.hand.iter().any(|card| card.instance_id == "hit"));
    assert_eq!(player.library.len(), library_size - 1);
    assert_eq!(
        player.library.first().map(|card| card.instance_id.as_str()),
        Some("miss")
    );
}

/// Manifest dread (CR 701.62a, 701.40a): one of the top two cards enters face
/// down as a 2/2 creature and the other goes to the graveyard.
#[test]
fn manifest_dread_puts_one_card_face_down_and_the_other_into_the_graveyard() {
    let rules = parsed_rules(
        "Creature - Rat",
        "When this creature dies, manifest dread. (Look at the top two cards of your library. Put one onto the battlefield face down as a 2/2 creature and the other into your graveyard. Turn it face up any time for its mana cost if it's a creature card.)",
    );
    let mut engine = test_engine(2);
    let library_size = engine.state.players[0].library.len();
    let graveyard_size = engine.state.players[0].graveyard.len();
    let source = creature("beastie", "player-0");
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&source, BTreeMap::new()),
    );
    let player = &engine.state.players[0];
    assert_eq!(player.library.len(), library_size - 2);
    assert_eq!(player.graveyard.len(), graveyard_size + 1);
    let manifested = player
        .battlefield
        .iter()
        .find(|card| card.flags.get("faceDown") == Some(&true))
        .expect("a face-down permanent");
    assert_eq!(manifested.definition.type_line, "Creature");
    assert_eq!(manifested.definition.power.as_deref(), Some("2"));
    assert!(
        manifested
            .printed_definition
            .as_ref()
            .is_some_and(|printed| has_card_type(printed, "Land"))
    );
}

/// Bolster N (CR 701.39a) chooses among the creatures with the least current
/// toughness only.
#[test]
fn bolster_counters_a_creature_with_the_least_toughness() {
    let rules = parsed_rules(
        "Creature - Bird Soldier",
        "When this creature enters, bolster 2. (Choose a creature with the least toughness among creatures you control and put two +1/+1 counters on it.)",
    );
    let mut engine = test_engine(2);
    let mut big = creature("big", "player-0");
    big.definition.toughness = Some("5".to_string());
    let small = creature("small", "player-0");
    engine.state.players[0].battlefield.push(big.clone());
    engine.state.players[0].battlefield.push(small);
    resolve(&mut engine, &rules[0], &stack_object(&big, BTreeMap::new()));
    assert_eq!(plus_one(&engine, "small"), 2);
    assert_eq!(plus_one(&engine, "big"), 0);
}

/// Suspect over the ability's own source (CR 701.60a), applied once (701.60d).
#[test]
fn suspect_it_designates_the_source_once() {
    let rules = parsed_rules(
        "Creature - Construct",
        "When this creature enters, suspect it. (It has menace and can't block.)",
    );
    let mut engine = test_engine(2);
    let servitor = creature("servitor", "player-0");
    engine.state.players[0].battlefield.push(servitor.clone());
    resolve(
        &mut engine,
        &rules[0],
        &stack_object(&servitor, BTreeMap::new()),
    );
    let rules_after_first = on_battlefield(&engine, "servitor").definition.rules.len();
    assert_eq!(
        on_battlefield(&engine, "servitor").flags.get("suspected"),
        Some(&true)
    );
    let stack = stack_object(on_battlefield(&engine, "servitor"), BTreeMap::new());
    resolve(&mut engine, &rules[0], &stack);
    assert_eq!(
        on_battlefield(&engine, "servitor").definition.rules.len(),
        rules_after_first
    );
}
