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

/// Runs one effect list as the ability of the `source` permanent.
fn resolve(
    engine: &mut GameEngine,
    source: &str,
    rule: &Value,
    mut bindings: BTreeMap<String, RuntimeBinding>,
) {
    let stack = stack_for(engine, source, rule);
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the token effects resolve");
}

fn created_tokens(engine: &GameEngine) -> Vec<CardInstance> {
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.is_token)
        .cloned()
        .collect()
}

fn written_token(power: Value, subtype: &str) -> Value {
    json!({
        "kind": "createTokens",
        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
        "quantity": { "kind": "integer", "value": 1 },
        "token": {
            "power": power,
            "toughness": power,
            "colors": ["green"],
            "types": ["Creature"],
            "subtypes": [subtype],
            "abilities": [],
        },
    })
}

/// CR 111.4, 107.3: an `X/X` description keeps its power and toughness as
/// shared numeric expressions, evaluated as the token is created; a
/// coordinated list is one ordinary creation per description.
#[test]
fn written_token_statistics_are_numeric_expressions_and_lists_create_each_description() {
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [
            written_token(json!({ "kind": "boundValue", "id": "x" }), "Ooze"),
            written_token(json!({ "kind": "integer", "value": 2 }), "Wolf"),
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    add(&mut engine, "summons", "Sorcery", "0", Vec::new());
    resolve(
        &mut engine,
        "summons",
        &rule,
        BTreeMap::from([("x".to_string(), RuntimeBinding::Integer(3))]),
    );
    let stats = created_tokens(&engine)
        .iter()
        .map(|token| {
            (
                token.definition.type_line.clone(),
                engine.current_power(token),
                engine.current_toughness(token),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stats,
        [
            ("Creature - Ooze".to_string(), 3, 3),
            ("Creature - Wolf".to_string(), 2, 2),
        ]
    );
}

/// CR 604.3, 208.2: a creature token described without power and toughness
/// has them defined by its characteristic-defining ability; a noncreature
/// token described without them has none.
#[test]
fn a_token_without_stated_statistics_reads_its_characteristic_defining_ability() {
    let cda = json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "characteristicDefining": true,
        "modifiers": [{
            "kind": "setBasePowerToughness",
            "objects": { "kind": "self" },
            "layer": "7a",
            "power": { "kind": "integer", "value": 4 },
            "toughness": { "kind": "integer", "value": 4 },
        }],
    });
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "createTokens",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "quantity": { "kind": "integer", "value": 1 },
                "token": {
                    "colors": ["green", "white"],
                    "types": ["Creature"],
                    "subtypes": ["Elemental"],
                    "abilities": [cda],
                },
            },
            {
                "kind": "createTokens",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "quantity": { "kind": "integer", "value": 1 },
                "tapped": true,
                "token": {
                    "colors": [],
                    "types": ["Land"],
                    "subtypes": ["Planet"],
                    "name": "New Planet",
                    "abilities": [],
                },
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    add(&mut engine, "resurgence", "Instant", "0", Vec::new());
    resolve(&mut engine, "resurgence", &rule, BTreeMap::new());
    let tokens = created_tokens(&engine);
    assert_eq!(tokens.len(), 2);
    assert_eq!(engine.current_power(&tokens[0]), 4);
    assert_eq!(engine.current_toughness(&tokens[0]), 4);
    assert_eq!(tokens[1].definition.name, "New Planet");
    assert_eq!(tokens[1].definition.type_line, "Land - Planet");
    assert_eq!(tokens[1].definition.power, None);
    assert!(tokens[1].tapped);
}

/// CR 508.4: a token put onto the battlefield attacking during combat is an
/// attacking creature with a chosen defender, without being declared; outside
/// combat it enters but is not attacking.
#[test]
fn tokens_entering_attacking_join_combat_only_during_combat() {
    let mut creation = written_token(json!({ "kind": "integer", "value": 1 }), "Goblin");
    creation["quantity"] = json!({ "kind": "integer", "value": 2 });
    creation["tapped"] = json!(true);
    creation["attacking"] = json!(true);
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [creation],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut outside = engine();
    add(&mut outside, "ascendancy", "Enchantment", "0", Vec::new());
    outside.state.step = GameStep::PostcombatMain;
    resolve(&mut outside, "ascendancy", &rule, BTreeMap::new());
    assert!(outside.state.combat.attackers.is_empty());
    assert!(
        created_tokens(&outside)
            .iter()
            .all(|token| token.tapped && !token.flags.contains_key("attacking"))
    );

    let mut combat = engine();
    add(&mut combat, "ascendancy", "Enchantment", "0", Vec::new());
    combat.state.step = GameStep::DeclareAttackers;
    resolve(&mut combat, "ascendancy", &rule, BTreeMap::new());
    let tokens = created_tokens(&combat);
    assert_eq!(tokens.len(), 2);
    for token in &tokens {
        assert_eq!(token.flags.get("attacking"), Some(&true));
        let assignment = combat
            .state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == token.instance_id)
            .expect("the token is an attacking creature");
        assert_eq!(assignment.defending_player_id, "player-1");
    }
}

/// CR 106.6: a barred cast use (`This mana can't be spent to cast a
/// nonartifact spell`) bars only the matching casts; the mana still pays for
/// artifact spells and for anything that is not a cast.
#[test]
fn a_barred_cast_use_spares_every_other_payment() {
    let restriction = json!({
        "kind": "castSpellProhibited",
        "where": {
            "kind": "not",
            "operand": { "kind": "cardTypeContains", "value": "Artifact" },
        },
    });
    let mana = FloatingMana {
        symbol: "C".to_string(),
        spend_restriction: Some(restriction.clone()),
    };
    let artifact = definition("relic", "Artifact", "0", Vec::new());
    let creature = definition("bear", "Creature - Bear", "2", Vec::new());
    let can_pay = |card: Option<&CardDefinition>| {
        super::super::super::mana_postcost_ordered::mana_pool_entry_can_pay(&mana, card, None, None)
    };
    assert!(can_pay(Some(&artifact)));
    assert!(!can_pay(Some(&creature)));
    assert!(can_pay(None), "an activation or another cost is not a cast");
    let rule = json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "addMana",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "mana": "{C}",
            "spendRestriction": restriction,
        }],
    });
    assert!(rule_is_executable(&rule), "{rule}");
}
