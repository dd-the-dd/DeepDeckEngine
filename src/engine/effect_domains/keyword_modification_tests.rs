//! Runtime proofs of resolved keyword and ability loss over the shared
//! battlefield object operand: a declared target loses a keyword only until
//! end of turn (Radjan Spirit), a bound single object, a bound collection
//! and the source itself (an indefinite loss that a later grant overrides),
//! a collection losing all abilities (Wrath of Oko), and a card-zone operand
//! that stays explicitly unsupported.
use super::super::super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("keyword test decision has no option"))
    }
}

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{1}{U}".to_string(),
        name: id.to_string(),
        power: Some("2".to_string()),
        rules,
        toughness: Some("2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn keyword(kind: &str) -> Value {
    json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": { "kind": kind } })
}

fn engine() -> GameEngine {
    let mut filler = definition("keyword-filler", "Basic Land - Island", Vec::new());
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
        311,
    )
    .expect("keyword test setup is valid")
}

/// Puts a permanent on player-0's battlefield and returns its instance id.
fn add(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) -> String {
    let mut card = card_instance("player-0", &definition(id, type_line, rules), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    id.to_string()
}

fn permanent(engine: &GameEngine, id: &str) -> CardInstance {
    let (player, index) = engine.permanent_position(id).expect("permanent exists");
    engine.state.players[player].battlefield[index].clone()
}

fn has(engine: &GameEngine, id: &str, keyword: &str) -> bool {
    engine.permanent_has_effective_keyword(&permanent(engine, id), keyword)
}

fn stack_for(engine: &GameEngine, source: &str, rule: &Value) -> StackObject {
    StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: permanent(engine, source),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: value_kind(rule).map(ToOwned::to_owned),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn resolve(
    engine: &mut GameEngine,
    rule: &Value,
    stack: &StackObject,
    bindings: &mut BTreeMap<String, RuntimeBinding>,
) {
    engine
        .execute_effects(
            &rule["effects"],
            stack,
            bindings,
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the effects resolve");
}

/// Radjan Spirit, as the Oracle compiler emits it.
fn radjan_spirit_rule() -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseTargets",
                "id": "targetPermanent",
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "effects": [{
            "kind": "removeKeywordsFromPermanents",
            "keywords": ["flying"],
            "objects": { "kind": "chosenTarget", "id": "targetPermanent" },
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }],
    })
}

#[test]
fn radjan_spirit_removes_flying_from_the_chosen_target_only_until_end_of_turn() {
    let mut engine = engine();
    let rule = radjan_spirit_rule();
    assert!(rule_is_executable(&rule), "{rule}");
    let spirit = add(
        &mut engine,
        "radjan-spirit",
        "Creature - Spirit",
        vec![rule.clone()],
    );
    let target = add(
        &mut engine,
        "target-bird",
        "Creature - Bird",
        vec![keyword("flying")],
    );
    let other = add(
        &mut engine,
        "other-bird",
        "Creature - Bird",
        vec![keyword("flying")],
    );
    let mut stack = stack_for(&engine, &spirit, &rule);
    stack.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: target.clone(),
        },
    );
    resolve(&mut engine, &rule, &stack, &mut BTreeMap::new());

    assert!(!has(&engine, &target, "flying"), "the target lost flying");
    assert!(
        has(&engine, &other, "flying"),
        "only the target is affected"
    );
    // The next turn the loss has ended (CR 514.2, 611.2a).
    engine.state.turn_number += 1;
    assert!(has(&engine, &target, "flying"));
}

#[test]
fn bound_objects_and_the_source_lose_keywords_with_their_duration() {
    // (operand, binding, duration, keyword): Gravity Well's bound attacker,
    // Whiteout's bound collection, Elder Land Wurm's source.
    let cases = [
        (
            json!({ "kind": "boundObjects", "binding": "characteristicObjects0" }),
            Some(RuntimeBinding::Target(TargetRef::Permanent {
                instance_id: "subject".to_string(),
            })),
            "untilEndOfCurrentTurn",
            "flying",
        ),
        (
            json!({ "kind": "boundObjects", "binding": "characteristicObjects0" }),
            Some(RuntimeBinding::Objects(vec!["subject".to_string()])),
            "untilEndOfCurrentTurn",
            "flying",
        ),
        (json!({ "kind": "self" }), None, "permanent", "defender"),
    ];
    for (objects, binding, duration, lost) in cases {
        let mut engine = engine();
        let rule = json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "removeKeywordsFromPermanents",
                "keywords": [lost],
                "objects": objects,
                "duration": { "kind": duration },
            }],
        });
        assert!(rule_is_executable(&rule), "{rule}");
        let subject = add(
            &mut engine,
            "subject",
            "Creature - Wall",
            vec![keyword(lost)],
        );
        let stack = stack_for(&engine, &subject, &rule);
        let mut bindings = binding
            .map(|binding| BTreeMap::from([("characteristicObjects0".to_string(), binding)]))
            .unwrap_or_default();
        resolve(&mut engine, &rule, &stack, &mut bindings);
        assert!(!has(&engine, &subject, lost), "{duration} {lost}");

        engine.state.turn_number += 1;
        assert_eq!(
            has(&engine, &subject, lost),
            duration == "untilEndOfCurrentTurn",
            "an indefinite loss lasts while the permanent stays"
        );
    }
}

#[test]
fn a_later_resolved_grant_restores_an_indefinitely_lost_keyword() {
    let mut engine = engine();
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "removeKeywordsFromPermanents",
                "keywords": ["defender"],
                "objects": { "kind": "self" },
                "duration": { "kind": "permanent" },
            },
            {
                "kind": "grantKeyword",
                "keyword": "defender",
                "object": { "kind": "self" },
                "duration": { "kind": "permanent" },
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let wall = add(
        &mut engine,
        "wall",
        "Creature - Wall",
        vec![keyword("defender")],
    );
    let stack = stack_for(&engine, &wall, &rule);
    resolve(&mut engine, &rule, &stack, &mut BTreeMap::new());
    // CR 613.7: the grant has the later timestamp.
    assert!(has(&engine, &wall, "defender"));
}

#[test]
fn every_creature_of_a_bound_collection_loses_all_abilities_indefinitely() {
    let mut engine = engine();
    // Wrath of Oko's first two instructions.
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "bind",
                "id": "characteristicObjects0",
                "value": {
                    "kind": "eachPermanent",
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            },
            {
                "kind": "loseAllAbilitiesPermanently",
                "object": { "kind": "boundObjects", "binding": "characteristicObjects0" },
                "duration": { "kind": "permanent" },
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let source = add(&mut engine, "oko-source", "Sorcery", Vec::new());
    let first = add(
        &mut engine,
        "first",
        "Creature - Bird",
        vec![keyword("flying")],
    );
    let second = add(
        &mut engine,
        "second",
        "Creature - Cat",
        vec![keyword("vigilance")],
    );
    let land = add(&mut engine, "land", "Land", vec![keyword("hexproof")]);
    let stack = stack_for(&engine, &source, &rule);
    resolve(&mut engine, &rule, &stack, &mut BTreeMap::new());
    assert!(!has(&engine, &first, "flying"));
    assert!(!has(&engine, &second, "vigilance"));
    assert!(
        has(&engine, &land, "hexproof"),
        "a noncreature is not affected"
    );
    engine.state.turn_number += 1;
    assert!(!has(&engine, &first, "flying"));
}

#[test]
fn the_source_gains_a_triggered_ability_until_end_of_turn() {
    // Reckless Pyrosurfer: `this creature gains battle cry until end of turn`.
    let granted = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "declaredAttacker", "object": { "kind": "self" } },
        "effects": [{
            "kind": "modifyPowerToughness",
            "object": {
                "kind": "eachPermanent",
                "excludeSource": true,
                "where": { "kind": "and", "operands": [
                    { "kind": "isAttacking" },
                    { "kind": "cardTypeContains", "value": "Creature" },
                ] },
            },
            "power": { "kind": "integer", "value": 1 },
            "toughness": { "kind": "integer", "value": 0 },
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }],
    });
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "permanentEntered",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": { "kind": "cardTypeContains", "value": "Land" },
        },
        "effects": [{
            "kind": "grantAbility",
            "ability": granted,
            "object": { "kind": "self" },
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let surfer = add(
        &mut engine,
        "surfer",
        "Creature - Human",
        vec![rule.clone()],
    );
    let stack = stack_for(&engine, &surfer, &rule);
    resolve(&mut engine, &rule, &stack, &mut BTreeMap::new());
    let rules = permanent(&engine, &surfer)
        .definition
        .printed_rules()
        .to_vec();
    assert!(rules.iter().any(|rule| {
        value_kind(rule) == Some("triggeredAbility")
            && rule["temporaryUntilTurn"].as_u64() == Some(u64::from(engine.state.turn_number))
    }));
}

#[test]
fn keyword_loss_over_a_card_zone_or_an_unknown_duration_stays_unsupported() {
    let loss = |objects: Value, duration: &str| {
        json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "removeKeywordsFromPermanents",
                "keywords": ["flying"],
                "objects": objects,
                "duration": { "kind": duration },
            }],
        })
    };
    let graveyard = json!({
        "kind": "cardsInZone",
        "zone": {
            "kind": "graveyard",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
    });
    assert!(!rule_is_executable(&loss(
        graveyard,
        "untilEndOfCurrentTurn"
    )));
    assert!(!rule_is_executable(&loss(
        json!({ "kind": "triggeringCard" }),
        "untilEndOfCurrentTurn"
    )));
    assert!(!rule_is_executable(&loss(
        json!({ "kind": "self" }),
        "untilYourNextTurn"
    )));
    // The same operand on the battlefield is supported.
    assert!(rule_is_executable(&loss(
        json!({ "kind": "self" }),
        "untilEndOfCurrentTurn"
    )));
}
