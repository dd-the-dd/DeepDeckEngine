//! Replacement vocabulary at runtime (CR 614.1a, 616.1, 611.2a): the
//! generalized graveyard-destination replacement (dying, destinations other
//! than exile, its reflexive ability), the effect-created replacements over
//! the objects a spell damaged, and effect-created quantity replacements
//! ordered with the static ones by the affected player.
use super::super::quantity_replacement::{QuantityEvent, QuantityOrigin, QuantityRecipient};
use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn battlefield_static(modifier: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [modifier],
    })
}

/// `If a creature an opponent controls would die, exile it instead.`
fn opponent_creature_dies_exiled() -> Value {
    json!({
        "kind": "replaceGraveyardDestination",
        "graveyardOwners": { "kind": "eachPlayer" },
        "to": { "kind": "exile" },
        "permanentWhere": { "kind": "cardTypeContains", "value": "Creature" },
        "controller": { "kind": "opponentsOf", "player": controller() },
        "from": { "kind": "battlefield" },
    })
}

fn install_source(engine: &mut GameEngine, id: &str, modifier: Value) {
    let mut definition = test_definition(id, "Creature");
    definition.rules = vec![battlefield_static(modifier)];
    assert!(rule_is_executable(&definition.rules[0]), "{id}");
    engine.state.players[0]
        .battlefield
        .push(test_instance(id, definition, "player-0"));
    admit(engine);
}

/// Fixture permanents enter through the same incarnation admission a real
/// zone change performs, so later moves can resolve their identity.
fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn creature(engine: &mut GameEngine, player: usize, id: &str) {
    let owner = format!("player-{player}");
    engine.state.players[player].battlefield.push(test_instance(
        id,
        test_definition(id, "Creature"),
        &owner,
    ));
    admit(engine);
}

fn in_zone(engine: &GameEngine, player: usize, zone: &str, id: &str) -> bool {
    let player = &engine.state.players[player];
    let cards = match zone {
        "exile" => &player.exile,
        "graveyard" => &player.graveyard,
        "hand" => &player.hand,
        "library" => &player.library,
        _ => &player.battlefield,
    };
    cards.iter().any(|card| card.instance_id == id)
}

fn died(engine: &GameEngine, id: &str) -> bool {
    engine
        .state
        .events
        .iter()
        .any(|event| event.kind == "permanentDied" && event.card_instance_id.as_deref() == Some(id))
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_opponents_dying_creature_is_exiled_and_does_not_die() {
    let mut engine = test_engine(2);
    install_source(&mut engine, "liesa", opponent_creature_dies_exiled());
    creature(&mut engine, 1, "theirs");
    creature(&mut engine, 0, "mine");
    engine
        .move_permanent_to_graveyard("theirs", "destroyed")
        .expect("the opponent's creature would die");
    engine
        .move_permanent_to_graveyard("mine", "destroyed")
        .expect("the controller's creature dies");
    assert!(in_zone(&engine, 1, "exile", "theirs"));
    assert!(!died(&engine, "theirs"), "an exile-instead is no death");
    assert!(in_zone(&engine, 0, "graveyard", "mine"));
    assert!(died(&engine, "mine"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn regeneration_prevents_the_destruction_an_exile_instead_would_follow() {
    let mut engine = test_engine(2);
    install_source(&mut engine, "liesa", opponent_creature_dies_exiled());
    creature(&mut engine, 1, "regenerating");
    engine.install_regeneration_shield("regenerating");
    engine
        .move_permanent_to_graveyard("regenerating", "destroyed")
        .expect("destruction is replaced by regeneration");
    assert!(
        in_zone(&engine, 1, "battlefield", "regenerating"),
        "regeneration replaces the destruction itself (CR 701.19a)"
    );
    // Without a shield the destruction happens and its zone change is
    // replaced: the permanent is exiled, destroyed but not dead.
    engine
        .move_permanent_to_graveyard("regenerating", "destroyed")
        .expect("the second destruction is not regenerated");
    assert!(in_zone(&engine, 1, "exile", "regenerating"));
    assert!(!died(&engine, "regenerating"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_source_replacement_moves_the_source_to_its_owners_library() {
    for (position, expected) in [("top", true), ("bottom", false)] {
        let mut engine = test_engine(2);
        install_source(
            &mut engine,
            "gravebane",
            json!({
                "kind": "replaceGraveyardDestination",
                "graveyardOwners": { "kind": "eachPlayer" },
                "object": { "kind": "self" },
                "from": { "kind": "battlefield" },
                "to": { "kind": "ownerLibrary", "position": position },
            }),
        );
        engine
            .move_permanent_to_graveyard("gravebane", "destroyed")
            .expect("the source would die");
        let library = &engine.state.players[0].library;
        let on_top = library
            .last()
            .is_some_and(|card| card.instance_id == "gravebane");
        let on_bottom = library
            .first()
            .is_some_and(|card| card.instance_id == "gravebane");
        assert_eq!(on_top, expected, "{position}");
        assert_eq!(on_bottom, !expected, "{position}");
        assert!(!died(&engine, "gravebane"));
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_reflexive_ability_triggers_when_the_replacement_applies() {
    let mut engine = test_engine(2);
    let mut modifier = opponent_creature_dies_exiled();
    modifier["onReplaced"] = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "reflexiveTriggerCreated", "object": { "kind": "self" } },
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": 1 },
        }],
    });
    install_source(&mut engine, "head-of-the-hunt", modifier);
    creature(&mut engine, 0, "mine");
    engine
        .move_permanent_to_graveyard("mine", "destroyed")
        .expect("the controller's own creature dies");
    assert!(
        engine.state.stack.is_empty(),
        "no replacement, no reflexive"
    );
    creature(&mut engine, 1, "theirs");
    engine
        .move_permanent_to_graveyard("theirs", "destroyed")
        .expect("the opponent's creature is exiled");
    assert_eq!(engine.state.stack.len(), 1, "When you do, ... triggered");
}

fn spell_stack(effects: Value) -> StackObject {
    StackObject {
        id: "stack:anger".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("anger", test_definition("anger", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": effects,
        })),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// `If a creature dealt damage this way would die this turn, exile it
/// instead.` follows only the permanents the resolving spell damaged.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn damaged_this_way_exiles_only_the_creatures_the_spell_damaged() {
    let mut engine = test_engine(2);
    creature(&mut engine, 1, "damaged");
    creature(&mut engine, 1, "spared");
    // An earlier, unrelated source's damage does not count.
    engine.record_event(
        "damageDealt",
        None,
        Some("spared".to_string()),
        json!({ "amount": 2, "source": "some-other-source", "combat": false }),
    );
    engine.record_event(
        "damageDealt",
        None,
        Some("damaged".to_string()),
        json!({ "amount": 3, "source": "anger", "combat": false }),
    );
    let effects = json!([{
        "kind": "installGraveyardDestinationReplacement",
        "replacement": {
            "kind": "replaceGraveyardDestination",
            "graveyardOwners": { "kind": "eachPlayer" },
            "to": { "kind": "exile" },
            "permanentWhere": { "kind": "cardTypeContains", "value": "Creature" },
            "from": { "kind": "battlefield" },
        },
        "duration": { "kind": "untilEndOfCurrentTurn" },
        "objects": { "kind": "damagedThisResolution" },
    }]);
    assert!(rule_is_executable(&json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": effects,
    })));
    let stack = spell_stack(effects.clone());
    execute_effects_at_resolution_start(
        &mut engine,
        &effects,
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut RandomAi::seeded(7),
    )
    .expect("the replacement is created");
    for id in ["damaged", "spared"] {
        engine
            .move_permanent_to_graveyard(id, "lethalDamage")
            .expect("the creature dies");
    }
    assert!(in_zone(&engine, 1, "exile", "damaged"));
    assert!(in_zone(&engine, 1, "graveyard", "spared"));
    // It lasts this turn only (CR 611.2a).
    engine.state.turn_number += 1;
    creature(&mut engine, 1, "later");
    engine.record_event(
        "damageDealt",
        None,
        Some("later".to_string()),
        json!({ "amount": 3, "source": "anger", "combat": false }),
    );
    engine
        .move_permanent_to_graveyard("later", "lethalDamage")
        .expect("the creature dies next turn");
    assert!(in_zone(&engine, 1, "graveyard", "later"));
}

/// Records each replacement-order request and picks the option whose source
/// is listed first.
struct PickInOrder {
    preferred: Vec<String>,
    requests: usize,
}
impl DecisionProvider for PickInOrder {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        assert_eq!(request.player_id, "player-0", "the affected player orders");
        self.requests += 1;
        for preferred in &self.preferred {
            if let Some(index) = request
                .options
                .iter()
                .position(|option| option.card_instance_id.as_deref() == Some(preferred))
            {
                return Ok(index);
            }
        }
        Ok(0)
    }
}

fn token_program(quantity: Value) -> Value {
    json!({
        "kind": "replaceEventQuantity",
        "event": {
            "kind": "tokenCreation",
            "origin": "any",
            "minimum": 1,
            "player": controller(),
        },
        "quantity": quantity,
    })
}

/// A static doubler and a Kaya-style created doubler (`Until end of turn,
/// if one or more tokens would be created under your control, twice that
/// many ...`) plus an additive one: the affected player orders them and the
/// result follows that order (CR 616.1).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn created_and_static_token_replacements_apply_in_the_affected_players_order() {
    let double = json!({
        "kind": "multiply",
        "left": { "kind": "integer", "value": 2 },
        "right": { "kind": "eventQuantity" },
    });
    let plus_one = json!({
        "kind": "add",
        "left": { "kind": "eventQuantity" },
        "right": { "kind": "integer", "value": 1 },
    });
    let mut results = Vec::new();
    for preferred in [["plus-one", "kaya"], ["kaya", "plus-one"]] {
        let mut engine = test_engine(2);
        install_source(&mut engine, "plus-one", token_program(plus_one.clone()));
        let kaya = test_instance(
            "kaya",
            test_definition("kaya", "Legendary Planeswalker \u{2014} Kaya"),
            "player-0",
        );
        engine.state.players[0].battlefield.push(kaya.clone());
        let effect = json!({
            "kind": "installQuantityReplacement",
            "program": token_program(double.clone()),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        });
        let mut stack = spell_stack(json!([effect.clone()]));
        stack.card = kaya;
        engine.install_created_quantity_replacement(&effect, &stack);
        let token = test_definition("token", "Token Creature \u{2014} Spirit");
        let mut provider = PickInOrder {
            preferred: preferred.iter().map(|id| id.to_string()).collect(),
            requests: 0,
        };
        let resolution = engine
            .resolve_quantity_event(
                QuantityEvent {
                    amount: 1,
                    origin: QuantityOrigin::Effect,
                    recipient: QuantityRecipient::Token {
                        controller: "player-0",
                        definition: &token,
                    },
                },
                &mut provider,
            )
            .expect("both replacements apply");
        assert_eq!(
            provider.requests, 1,
            "one ordering choice for two proposals"
        );
        results.push(resolution.amount);
    }
    // (1 + 1) * 2 = 4 when the addition applies first; 1 * 2 + 1 = 3 otherwise.
    assert_eq!(results, vec![4, 3]);
}

/// `{3}{R}, {T}: If a source you control would deal damage to an opponent
/// this turn, it deals double that damage to that player instead.`: the
/// created program reads its controller's relations and lasts this turn.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_created_damage_doubler_applies_to_its_controllers_sources_this_turn() {
    use super::super::damage_replacement::DamageEventView;
    use super::super::quantity_replacement::OptionalQuantityProvider;
    let mut engine = test_engine(2);
    creature(&mut engine, 0, "goliath");
    creature(&mut engine, 0, "attacker");
    creature(&mut engine, 1, "their-attacker");
    let effect = json!({
        "kind": "installDamageReplacement",
        "program": {
            "kind": "replaceDamage",
            "event": {
                "damage": "any",
                "source": { "kind": "objects", "controller": controller() },
                "recipients": [{
                    "kind": "player",
                    "player": { "kind": "opponentsOf", "player": controller() },
                }],
            },
            "replacement": {
                "kind": "modifyAmount",
                "quantity": {
                    "kind": "multiply",
                    "left": { "kind": "integer", "value": 2 },
                    "right": { "kind": "eventQuantity" },
                },
            },
        },
        "duration": { "kind": "untilEndOfCurrentTurn" },
    });
    assert!(super::super::damage_replacement::created_damage_replacement_supported(&effect));
    let mut stack = spell_stack(json!([effect.clone()]));
    stack.card = engine.state.players[0].battlefield[0].clone();
    engine.install_created_damage_replacement(&effect, &stack);
    let amount = |engine: &GameEngine, source: &str, controller: &str, player: &str| {
        let source_card = engine
            .permanent_position(source)
            .map(|(p, i)| engine.state.players[p].battlefield[i].clone());
        let target = TargetRef::Player {
            player_id: player.to_string(),
        };
        engine
            .apply_damage_replacements(
                &DamageEventView {
                    target: &target,
                    source_id: source,
                    source_card: source_card.as_ref(),
                    source_controller: Some(controller),
                    source_is_spell: false,
                    combat: false,
                },
                3,
                true,
                &mut OptionalQuantityProvider(None),
            )
            .expect("one program at most applies")
            .amount
    };
    assert_eq!(amount(&engine, "attacker", "player-0", "player-1"), 6);
    assert_eq!(
        amount(&engine, "their-attacker", "player-1", "player-0"),
        3,
        "an opponent's source is not the controller's"
    );
    assert_eq!(
        amount(&engine, "attacker", "player-0", "player-0"),
        3,
        "the controller is not an opponent"
    );
    engine.state.turn_number += 1;
    assert_eq!(
        amount(&engine, "attacker", "player-0", "player-1"),
        3,
        "the created replacement lasted this turn only"
    );
}
