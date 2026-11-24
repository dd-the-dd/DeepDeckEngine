//! Runtime proofs of the restriction reader: turn windows, counted limits,
//! keyword-defined once-per-object limits, any-player activation and spell
//! cast restrictions are legality metadata read by the engine, never effects.
use super::*;

fn test_definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": type_line.starts_with("Creature").then_some("2"),
        "toughness": type_line.starts_with("Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid")
}

fn test_instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn test_engine() -> GameEngine {
    let filler = test_definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1471,
    )
    .expect("standalone test engine is valid")
}

/// A free ability gaining its activator 1 life, with the given metadata.
fn restricted_rule(metadata: Value) -> Value {
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "payMana", "manaCost": "{0}" }],
        "declaration": { "kind": "castingDeclaration", "decisions": [] },
        "effects": [{ "kind": "gainLife", "player": { "kind": "abilityController" },
                      "amount": { "kind": "integer", "value": 1 } }],
    });
    for (key, value) in metadata.as_object().expect("metadata object") {
        rule[key] = value.clone();
    }
    rule
}

fn engine_with_source(rule: Value) -> GameEngine {
    let mut engine = test_engine();
    let mut source = test_definition("restricted-source", "Artifact");
    source.rules = vec![rule];
    assert!(rule_is_executable(&source.rules[0]));
    engine.state.players[0].battlefield =
        vec![test_instance("restricted-source", source, "player-0")];
    engine.state.players[0].hand.clear();
    engine.state.players[1].hand.clear();
    engine.state.active_player = 0;
    engine.state.step = GameStep::PrecombatMain;
    engine
}

fn activation_offered(engine: &GameEngine, player: usize) -> Option<LegalAction> {
    engine
        .legal_priority_actions(player)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("restricted-source")
        })
}

/// Goblin Firestarter: `Activate only during your turn, before attackers are
/// declared.` A window is a turn owner and a set of steps of that turn.
#[test]
fn turn_window_admits_only_its_steps_of_its_owner_turn() {
    let rule = restricted_rule(json!({
        "activationCondition": { "kind": "and", "operands": [
            { "kind": "duringControllerTurn", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            { "kind": "duringSteps", "steps": ["untap", "upkeep", "draw", "precombatMain", "beginningOfCombat"] },
        ] },
    }));
    let mut engine = engine_with_source(rule);
    for (step, offered) in [
        (GameStep::Upkeep, true),
        (GameStep::PrecombatMain, true),
        (GameStep::BeginningOfCombat, true),
        (GameStep::DeclareAttackers, false),
        (GameStep::CombatDamage, false),
        (GameStep::PostcombatMain, false),
    ] {
        engine.state.step = step.clone();
        assert_eq!(
            activation_offered(&engine, 0).is_some(),
            offered,
            "window membership of {step:?}"
        );
    }
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 1;
    assert!(
        activation_offered(&engine, 0).is_none(),
        "`your turn` excludes an opponent's turn"
    );
}

/// A window naming only a step the turn structure does not model is not
/// executable rather than silently never (or always) available.
#[test]
fn a_window_of_only_unmodeled_steps_is_not_executable() {
    let rule = restricted_rule(json!({
        "activationCondition": { "kind": "duringSteps", "steps": ["endOfCombat"] },
    }));
    assert!(!rule_is_executable(&rule));
    let rule = restricted_rule(json!({
        "activationCondition": { "kind": "duringSteps", "steps": ["declareBlockers", "lateStep"] },
    }));
    assert!(!rule_is_executable(&rule));
}

/// Exhaust (CR 702.177a): `Activate only once` for this object; a new turn
/// does not reset it, unlike `once each turn`.
#[test]
fn exhaust_ability_cannot_be_activated_twice_even_on_a_later_turn() {
    let rule = restricted_rule(json!({
        "abilityClass": "exhaust",
        "activationLimit": { "kind": "oncePerGameObjectAbility", "id": "exhaust" },
    }));
    let mut engine = engine_with_source(rule);
    let action = activation_offered(&engine, 0).expect("an unused exhaust ability is offered");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(7))
        .expect("the exhaust ability activates");
    assert!(activation_offered(&engine, 0).is_none());
    engine.state.stack.clear();
    engine.state.turn_number += 2;
    assert!(
        activation_offered(&engine, 0).is_none(),
        "an exhaust ability stays used on later turns"
    );
}

/// `Activate no more than twice each turn.`: a counted per-turn limit, keyed
/// by the ability and reset by a new turn.
#[test]
fn counted_turn_limit_allows_exactly_its_maximum_each_turn() {
    let rule = restricted_rule(json!({
        "activationLimit": { "kind": "timesPerTurn", "maximum": 2, "id": "activationRestriction" },
    }));
    let mut engine = engine_with_source(rule);
    for seed in 0..2 {
        let action = activation_offered(&engine, 0).expect("activation within the limit");
        engine
            .apply_priority_action(&action, &mut RandomAi::seeded(seed))
            .expect("activation within the limit succeeds");
        engine.state.stack.clear();
    }
    assert!(
        activation_offered(&engine, 0).is_none(),
        "a third activation exceeds twice"
    );
    engine.state.turn_number += 1;
    assert!(
        activation_offered(&engine, 0).is_some(),
        "the limit counts per turn"
    );
}

/// Vintara Elephant: `Any player may activate this ability.` The opponent
/// activates it and controls the resulting ability (CR 602.2).
#[test]
fn any_player_activation_is_offered_to_and_controlled_by_the_opponent() {
    let rule = restricted_rule(json!({ "anyPlayerMayActivate": true }));
    let mut engine = engine_with_source(rule);
    engine.state.priority_player = Some(1);
    let action = activation_offered(&engine, 1).expect("the opponent may activate it");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(3))
        .expect("a non-controller activation passes the controller guard");
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].controller, "player-1");
    let unrestricted = engine_with_source(restricted_rule(json!({})));
    assert!(
        activation_offered(&unrestricted, 1).is_none(),
        "without the permission only the controller activates"
    );
}

/// Rally the Troops family: `Cast this spell only during the declare
/// attackers step ...` is a static cast restriction of the card itself.
#[test]
fn spell_cast_restriction_withholds_the_cast_outside_its_window() {
    let mut engine = test_engine();
    let mut spell = test_definition("windowed-instant", "Instant");
    spell.mana_cost = "{0}".to_string();
    spell.rules = vec![
        json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                          "amount": { "kind": "integer", "value": 1 } }],
        }),
        json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": { "kind": "inZone", "object": { "kind": "self" },
                             "zone": { "kind": "stackOrCast" } },
            "modifiers": [{ "kind": "castOnlyIf", "object": { "kind": "self" },
                            "condition": { "kind": "duringSteps", "steps": ["declareAttackers"] } }],
        }),
    ];
    assert!(spell.rules.iter().all(rule_is_executable));
    engine.state.players[0].hand = vec![test_instance("windowed-instant", spell, "player-0")];
    engine.state.active_player = 0;
    let offered = |engine: &GameEngine| {
        engine.legal_priority_actions(0).into_iter().any(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("windowed-instant")
        })
    };
    engine.state.step = GameStep::PrecombatMain;
    assert!(!offered(&engine), "illegal in a main phase");
    engine.state.step = GameStep::DeclareAttackers;
    assert!(offered(&engine), "legal in the declare attackers step");
}

/// CR 601.3: a cast during resolution (Epic Experiment, cascade-like free
/// casts) still obeys the card's own `Cast this spell only ...` restriction.
#[test]
fn spell_cast_restriction_applies_to_casts_during_resolution() {
    let mut engine = test_engine();
    let mut spell = test_definition("windowed-instant", "Instant");
    spell.mana_cost = "{0}".to_string();
    spell.rules = vec![
        json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                          "amount": { "kind": "integer", "value": 1 } }],
        }),
        json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": { "kind": "inZone", "object": { "kind": "self" },
                             "zone": { "kind": "stackOrCast" } },
            "modifiers": [{ "kind": "castOnlyIf", "object": { "kind": "self" },
                            "condition": { "kind": "duringSteps", "steps": ["declareAttackers"] } }],
        }),
    ];
    let exiled = test_instance("windowed-instant", spell, "player-0");
    engine.state.players[0].exile = vec![exiled.clone()];
    engine.state.active_player = 0;
    let offered = |engine: &GameEngine| {
        !engine
            .cast_without_paying_actions_from_zone(
                0,
                std::slice::from_ref(&exiled),
                "duringResolution",
                "exile",
            )
            .expect("cast options are computed")
            .is_empty()
    };
    engine.state.step = GameStep::PrecombatMain;
    assert!(!offered(&engine), "a free cast in a main phase is withheld");
    engine.state.step = GameStep::DeclareAttackers;
    assert!(
        offered(&engine),
        "the same free cast is legal while attacking"
    );
}
