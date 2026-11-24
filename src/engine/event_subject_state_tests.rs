//! Runtime proofs of permanent-state predicates on trigger-event subjects:
//! counters (CR 122.1) and designations read on the battlefield object, or
//! on its last-known information when it died (CR 603.10a). Laid to Rest
//! triggers for a creature that died with a +1/+1 counter and not for one
//! without; the event shapes of the cohort (attack, combat damage, entry,
//! aggregate attack/damage) are executable; a state atom on an event whose
//! subject is not a permanent stays unsupported.
use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
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
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        603,
    )
    .expect("event subject test setup is valid")
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) -> String {
    let mut card = card_instance("player-0", &definition(id, type_line, rules), 0);
    card.instance_id = id.to_string();
    card.printed_definition = Some(card.definition.clone());
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    id.to_string()
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn creature_with(state_atom: Value) -> Value {
    json!({ "kind": "and", "operands": [
        { "kind": "cardTypeContains", "value": "Creature" },
        state_atom,
    ] })
}

fn plus_one_counter() -> Value {
    json!({ "kind": "hasCounter", "counter": "+1/+1" })
}

fn triggered(event: Value) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": event,
        "effects": [{
            "kind": "gainLife",
            "amount": { "kind": "integer", "value": 2 },
            "player": controller(),
        }],
    })
}

/// Laid to Rest: `Whenever a creature you control with a +1/+1 counter on
/// it dies, you gain 2 life.`
fn laid_to_rest() -> Value {
    triggered(json!({
        "kind": "permanentDied",
        "player": controller(),
        "where": creature_with(plus_one_counter()),
    }))
}

#[test]
fn laid_to_rest_triggers_for_a_creature_that_died_with_a_counter_only() {
    let rule = laid_to_rest();
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    add(
        &mut engine,
        "laid-to-rest",
        "Enchantment",
        vec![rule.clone()],
    );
    let countered = add(&mut engine, "countered", "Creature - Bear", Vec::new());
    let plain = add(&mut engine, "plain", "Creature - Bear", Vec::new());
    let (player, index) = engine.permanent_position(&countered).unwrap();
    engine.state.players[player].battlefield[index]
        .counters
        .insert("+1/+1".to_string(), 1);
    let source = engine.state.players[0].battlefield[0].clone();
    let stack = StackObject {
        id: "sacrifice-instruction".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .sacrifice_effect_collection(
            vec![plain.clone(), countered.clone()],
            &["player-0".to_string()],
            &stack,
        )
        .expect("both creatures die");
    let triggered = engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_rule.as_ref() == Some(&rule))
        .collect::<Vec<_>>();
    assert_eq!(triggered.len(), 1, "only the creature with a counter");
    assert_eq!(
        triggered[0].decisions["triggeringPermanentId"].as_str(),
        Some(countered.as_str())
    );
}

#[test]
fn state_predicates_of_the_cohorts_event_subjects_are_executable() {
    let attacker = |where_: Value| {
        json!({
            "kind": "eventPattern",
            "lemma": "attack",
            "subject": {
                "role": "attacker",
                "object": { "kind": "permanents", "controller": controller(), "where": where_ },
            },
        })
    };
    let combat_damage_source = |where_: Value| {
        json!({
            "kind": "damageDealtBySource",
            "combatOnly": true,
            "recipient": { "kind": "player", "player": { "kind": "eachPlayer" } },
            "source": { "kind": "permanents", "controller": controller(), "where": where_ },
        })
    };
    let events = [
        // Byrke, Tenured Inkcaster, Elite Scaleguard.
        attacker(creature_with(plus_one_counter())),
        // Bred for the Hunt, Oona's Blackguard; Phlox (any counter);
        // Aragorn (renowned).
        combat_damage_source(creature_with(plus_one_counter())),
        combat_damage_source(creature_with(json!({ "kind": "hasCounter" }))),
        combat_damage_source(creature_with(
            json!({ "kind": "hasDesignation", "designation": "renowned" }),
        )),
        // Necroskitter, The Scorpion God.
        json!({
            "kind": "permanentDied",
            "player": { "kind": "opponentsOf", "player": controller() },
            "where": creature_with(json!({ "kind": "hasCounter", "counter": "-1/-1" })),
        }),
        // Threats Around Every Corner.
        json!({
            "kind": "permanentEntered",
            "player": controller(),
            "where": { "kind": "hasDesignation", "designation": "faceDown" },
        }),
        // Metropolis Angel, Clandestine Meddler, Haliya.
        json!({
            "kind": "controlledCreaturesAttacked",
            "player": controller(),
            "where": creature_with(json!({ "kind": "hasCounter" })),
        }),
        json!({
            "kind": "controlledCreaturesAttacked",
            "player": controller(),
            "where": creature_with(json!({ "kind": "hasDesignation", "designation": "suspected" })),
        }),
        json!({
            "kind": "controlledCreaturesCombatDamageToPlayer",
            "player": controller(),
            "where": creature_with(plus_one_counter()),
        }),
    ];
    for event in events {
        let rule = triggered(event);
        assert!(rule_is_executable(&rule), "{rule}");
    }
}

#[test]
fn a_state_atom_on_a_non_permanent_event_subject_stays_unsupported() {
    // A drawn card is not a permanent: it has no counters to read.
    let drawn = triggered(json!({
        "kind": "cardDrawn",
        "player": controller(),
        "where": plus_one_counter(),
    }));
    assert!(!rule_is_executable(&drawn), "{drawn}");
    // An unknown designation is refused, not read as absent.
    let unknown = triggered(json!({
        "kind": "permanentDied",
        "player": controller(),
        "where": { "kind": "hasDesignation", "designation": "blessed" },
    }));
    assert!(!rule_is_executable(&unknown), "{unknown}");
}
