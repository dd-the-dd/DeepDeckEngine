//! Runtime proof of object conditions over the shared object query:
//! an object a cost paid with is read from the record its paying cost wrote
//! (CR 601.2h, 602.2b, 608.2h), and a relation leaf of the condition's
//! filter reads its anchor from the resolving ability (CR 608.2b). Every
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
fn rules(name: &str, type_line: &str, oracle_text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{R}".to_string()),
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
        id: "stack:probe".to_string(),
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

/// The ids the costs of `rule` choose objects by.
fn cost_choice_ids(rule: &Value) -> Vec<String> {
    rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|cost| {
            cost["permanent"]["id"]
                .as_str()
                .or_else(|| cost["card"]["id"].as_str())
                .map(ToOwned::to_owned)
        })
        .collect()
}

/// Declares `player` for the rule's first target that no cost chooses.
fn target_player(stack: &mut StackObject, rule: &Value, player: &str) {
    let costs = cost_choice_ids(rule);
    let id = rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|decision| decision["id"].as_str())
        .find(|id| !costs.iter().any(|cost| cost == id))
        .expect("a declared target");
    stack.targets.insert(
        id.to_string(),
        TargetRef::Player {
            player_id: player.to_string(),
        },
    );
}

fn run(engine: &mut GameEngine, stack: &StackObject) {
    let effects = stack.ability_rule.as_ref().unwrap()["effects"].clone();
    let mut decisions = BTreeMap::new();
    execute_effects_at_resolution_start(
        engine,
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut decisions,
        &mut FirstOption,
    )
    .expect("the compiled program executes");
}

const GRIMA: &str = "{T}, Sacrifice another creature: Target player loses 1 life. If the sacrificed creature was legendary, amass Orcs 2.";

/// Gríma's activation with `victim` sacrificed: the number of Armies its
/// controller has once the ability resolved.
fn grima_armies(victim_type_line: &str) -> usize {
    let rule = rules(
        "Gríma Wormtongue",
        "Legendary Creature — Human Advisor",
        GRIMA,
    )
    .remove(0);
    let mut engine = engine();
    let source = card_instance("player-0", &definition("grima", "Creature", "{2}{B}"), 940);
    let victim = card_instance(
        "player-0",
        &definition("victim", victim_type_line, "{1}"),
        941,
    );
    let victim_id = victim.instance_id.clone();
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].battlefield.push(victim);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let sacrifice = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|cost| value_kind(cost) == Some("sacrificePermanent"))
        .expect("a sacrifice cost");
    // The card composition proved the producing cost and asked it to record
    // its object.
    assert_eq!(sacrifice["bindObjectAs"], "sacrificedPermanentObject");
    let targets = BTreeMap::from([(
        sacrifice["permanent"]["id"].as_str().unwrap().to_string(),
        TargetRef::Permanent {
            instance_id: victim_id.clone(),
        },
    )]);
    let mut stack = stack_object(source, &rule);
    engine.record_cost_paid_objects(&rule["costs"], &targets, &mut stack.decisions);
    assert!(stack.decisions.contains_key("sacrificedPermanentObject"));
    // The cost is paid: the object left the battlefield before resolution.
    engine
        .move_permanent_to_graveyard(&victim_id, "sacrificed")
        .expect("the sacrifice");
    target_player(&mut stack, &rule, "player-1");
    run(&mut engine, &stack);
    assert_eq!(engine.state.players[1].life, 19);
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.type_line.contains("Army"))
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn grima_reads_legendary_from_the_sacrificed_creatures_last_known_information() {
    assert_eq!(grima_armies("Legendary Creature — Human Noble"), 1);
    // Sibling: the same activation with a nonlegendary creature amasses no Army.
    assert_eq!(grima_armies("Creature — Human Soldier"), 0);
}

const ANARCH: &str = "{2}{R}, Discard a card at random: This creature deals 2 damage to any target. If the discarded card was multicolored, this creature deals 4 damage instead.";

/// Stormscale Anarch's activation discarding a card of `mana_cost` at
/// random: the damage its target player was dealt.
fn anarch_damage(mana_cost: &str) -> i32 {
    let rule = rules("Stormscale Anarch", "Creature — Viashino Shaman", ANARCH).remove(0);
    let discard = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|cost| value_kind(cost) == Some("discardCards"))
        .expect("a random discard cost");
    assert_eq!(discard["bindObjectAs"], "discardedCardObject");
    let mut engine = engine();
    let source = card_instance("player-0", &definition("anarch", "Creature", "{2}{R}"), 950);
    let discarded = card_instance(
        "player-0",
        &definition("discarded", "Instant", mana_cost),
        951,
    );
    let discarded_id = discarded.instance_id.clone();
    // The payment already moved the selected card to the graveyard.
    engine.state.players[0].graveyard.push(discarded);
    engine.state.players[0].battlefield.push(source.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let mut stack = stack_object(source.clone(), &rule);
    target_player(&mut stack, &rule, "player-1");
    engine.state.stack.push(stack);
    engine.record_event(
        "cardsRandomlySelected",
        Some("player-0".to_string()),
        Some(source.instance_id.clone()),
        json!({ "cardIds": [discarded_id], "minimum": 1, "maximum": 1, "reason": "cost" }),
    );
    engine.record_random_discard_cost_object(&rule);
    let stack = engine.state.stack.pop().expect("the activation");
    assert!(stack.decisions.contains_key("discardedCardObject"));
    run(&mut engine, &stack);
    20 - engine.state.players[1].life
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn stormscale_anarch_deals_four_only_for_a_multicolored_discarded_card() {
    assert_eq!(anarch_damage("{R}{G}"), 4);
    assert_eq!(anarch_damage("{R}"), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_cost_paid_object_without_its_record_is_an_error() {
    let rule = rules("Stormscale Anarch", "Creature — Viashino Shaman", ANARCH).remove(0);
    let engine = engine();
    let source = card_instance("player-0", &definition("anarch", "Creature", "{2}{R}"), 952);
    let stack = stack_object(source, &rule);
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": { "kind": "decisionResult", "decisionId": "discardedCardObject" },
        "where": { "kind": "colorContains", "value": "red" },
    });
    assert!(condition_supported(&condition));
    assert!(
        engine
            .checked_object_condition_matches(
                &condition,
                &stack,
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .is_err()
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_shared_creature_type_reads_the_looked_at_card_against_the_source() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("consul", "Creature — Kithkin Shaman", "{4}{R}"),
        960,
    );
    let kin = card_instance(
        "player-0",
        &definition("kin", "Creature — Kithkin Soldier", "{W}"),
        961,
    );
    let stranger = card_instance(
        "player-0",
        &definition("stranger", "Creature — Elf", "{G}"),
        962,
    );
    let (kin_id, stranger_id) = (kin.instance_id.clone(), stranger.instance_id.clone());
    engine.state.players[0].library.insert(0, kin);
    engine.state.players[0].library.insert(1, stranger);
    engine.state.players[0].battlefield.push(source.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": { "kind": "boundObjects", "binding": "lookedCards" },
        "where": {
            "kind": "sharesCharacteristic",
            "characteristic": "creatureType",
            "object": { "kind": "self" },
        },
    });
    assert!(condition_supported(&condition));
    let stack = StackObject {
        id: "stack:kinship".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let looked = |id: &str| {
        BTreeMap::from([(
            "lookedCards".to_string(),
            RuntimeBinding::Objects(vec![id.to_string()]),
        )])
    };
    assert!(
        engine
            .checked_object_condition_matches(
                &condition,
                &stack,
                &looked(&kin_id),
                &BTreeMap::new()
            )
            .expect("a prepared relation")
    );
    assert!(
        !engine
            .checked_object_condition_matches(
                &condition,
                &stack,
                &looked(&stranger_id),
                &BTreeMap::new()
            )
            .expect("a prepared relation")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn possessed_characteristics_and_relations_compile_to_executable_rules() {
    // Each rule is asserted canonical and executable by `rules`.
    rules(
        "Ensign Harry Kim, Operations Officer",
        "Legendary Creature — Human Engineer",
        "Whenever another creature you control enters, if it has greater power than Ensign Kim, put a +1/+1 counter on Ensign Kim.",
    );
    rules(
        "Hexgold Slash",
        "Instant",
        "Hexgold Slash deals 2 damage to target creature. If that creature has toxic, Hexgold Slash deals 4 damage to that creature instead.",
    );
    // A comparison with the source's own property, read on resolution.
    rules(
        "Sharp-Eyed Rookie",
        "Creature — Human Detective",
        "Whenever a creature you control enters, if its power is greater than this creature's power or its toughness is greater than this creature's toughness, put a +1/+1 counter on this creature and investigate.",
    );
    // A relation leaf over the looked-at card, prepared on resolution.
    rules(
        "Amareth, the Lustrous",
        "Legendary Creature — Dragon",
        "Whenever another permanent you control enters, look at the top card of your library. If it shares a card type with that permanent, you may reveal that card and put it into your hand.",
    );
    // No-code combination: an effect's exile result read like a cost's.
    rules(
        "Exiled Card Probe",
        "Sorcery",
        "Exile target card from a graveyard. If the exiled card was an artifact card, draw a card.",
    );
}
