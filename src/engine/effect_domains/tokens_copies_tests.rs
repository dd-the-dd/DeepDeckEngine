//! Runtime proofs of `becomes a copy of` over the shared battlefield object
//! operand (CR 707.2, 611.2c): each creature of a bound collection becomes a
//! copy of a target until end of turn while the copied object keeps its own
//! values (Mirrorweave), and a bound single object becomes a copy of the
//! source (Permeating Mass). Resolved characteristic markers of the copied
//! object are not copiable values.
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
            .ok_or_else(|| EngineError::new("copy test decision has no option"))
    }
}

fn definition(id: &str, type_line: &str, power: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{2}{U}".to_string(),
        name: id.to_string(),
        power: Some(power.to_string()),
        rules,
        toughness: Some(power.to_string()),
        type_line: type_line.to_string(),
    }
}

fn engine() -> GameEngine {
    let mut filler = definition("copy-filler", "Basic Land - Island", "0", Vec::new());
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
        707,
    )
    .expect("copy test setup is valid")
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str, power: &str, rules: Vec<Value>) {
    let mut card = card_instance("player-0", &definition(id, type_line, power, rules), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn permanent(engine: &GameEngine, id: &str) -> CardInstance {
    let (player, index) = engine.permanent_position(id).expect("permanent exists");
    engine.state.players[player].battlefield[index].clone()
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

#[test]
fn each_creature_of_a_bound_collection_becomes_a_copy_of_the_target_until_end_of_turn() {
    // Mirrorweave, as the Oracle compiler emits it.
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseTargets",
                "id": "targetPermanent1",
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "and", "operands": [
                        { "kind": "not", "operand": { "kind": "isLegendary" } },
                        { "kind": "cardTypeContains", "value": "Creature" },
                    ] },
                },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "effects": [
            {
                "kind": "bind",
                "id": "characteristicObjects0",
                "value": {
                    "kind": "eachPermanent",
                    "excludeSource": true,
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            },
            {
                "kind": "becomeCopyOfPermanent",
                "object": { "kind": "boundObjects", "binding": "characteristicObjects0" },
                "copy": { "kind": "chosenTarget", "id": "targetPermanent1" },
                "duration": { "kind": "untilEndOfCurrentTurn" },
                "retainResolvingAbility": false,
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    add(&mut engine, "mirrorweave", "Instant", "0", Vec::new());
    add(
        &mut engine,
        "model",
        "Creature - Elephant",
        "5",
        // A resolved color change of the model is not copied.
        vec![
            json!({ "kind": super::super::characteristic_layers::COLOR_SET_MARKER, "colors": ["B"] }),
        ],
    );
    add(&mut engine, "first", "Creature - Bear", "2", Vec::new());
    add(&mut engine, "second", "Creature - Bird", "1", Vec::new());
    add(&mut engine, "land", "Land", "0", Vec::new());
    let mut stack = stack_for(&engine, "mirrorweave", &rule);
    stack.targets.insert(
        "targetPermanent1".to_string(),
        TargetRef::Permanent {
            instance_id: "model".to_string(),
        },
    );
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the copy effects resolve");

    for copy in ["first", "second"] {
        let copy = permanent(&engine, copy);
        assert_eq!(copy.definition.name, "model");
        assert_eq!(engine.current_power(&copy), 5);
        assert_eq!(super::super::marker_colors(&copy.definition), None);
    }
    assert_eq!(permanent(&engine, "land").definition.name, "land");
    assert_eq!(
        permanent(&engine, "model").definition.name,
        "model",
        "the copied object is unchanged"
    );

    engine.cleanup(&mut FirstOption).expect("cleanup runs");
    assert_eq!(permanent(&engine, "first").definition.name, "first");
    assert_eq!(permanent(&engine, "second").definition.name, "second");
}

#[test]
fn a_bound_damaged_creature_becomes_a_copy_of_the_source() {
    // Permeating Mass: `that creature becomes a copy of this creature`.
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "damageDealtBySource",
            "combatOnly": true,
            "source": { "kind": "self" },
            "recipient": {
                "kind": "permanent",
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
        },
        "effects": [
            {
                "kind": "bind",
                "id": "characteristicObjects0",
                "value": { "kind": "triggeringPermanent" },
            },
            {
                "kind": "becomeCopyOfPermanent",
                "object": { "kind": "boundObjects", "binding": "characteristicObjects0" },
                "copy": { "kind": "self" },
                "retainResolvingAbility": false,
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    add(
        &mut engine,
        "mass",
        "Creature - Spirit",
        "1",
        vec![rule.clone()],
    );
    add(&mut engine, "victim", "Creature - Giant", "6", Vec::new());
    let stack = stack_for(&engine, "mass", &rule);
    let mut bindings = BTreeMap::from([(
        "characteristicObjects0".to_string(),
        RuntimeBinding::Target(TargetRef::Permanent {
            instance_id: "victim".to_string(),
        }),
    )]);
    engine
        .execute_effects(
            &json!([rule["effects"][1]]),
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the copy effect resolves");
    let victim = permanent(&engine, "victim");
    assert_eq!(victim.definition.name, "mass");
    assert_eq!(engine.current_power(&victim), 1);
    engine.state.turn_number += 1;
    assert_eq!(
        permanent(&engine, "victim").definition.name,
        "mass",
        "a copy effect without duration lasts"
    );
}

#[test]
fn copying_from_a_card_zone_collection_stays_unsupported() {
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "becomeCopyOfPermanent",
            "object": {
                "kind": "cardsInZone",
                "zone": {
                    "kind": "graveyard",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
            },
            "copy": { "kind": "self" },
            "retainResolvingAbility": false,
        }],
    });
    assert!(!rule_is_executable(&rule), "{rule}");
}
