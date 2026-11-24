//! Runtime proofs of the cast/play frame's resolution-time primitives:
//! optional versus mandatory casts (CR 608.2g), the graveyard exile
//! replacement of the spells cast (CR 614.1a), card copies cast during
//! resolution (CR 707.12, 707.10a), a choice among the cards a player owns
//! outside the game (CR 400.11a) and a random choice replayed by its seed.
use super::super::super::*;

/// Casts while a cast is offered and `casts` remain, then takes the
/// finishing option; records whether each request offered a way to decline.
struct CastThenFinish {
    casts: usize,
    declinable: Vec<bool>,
}

impl DecisionProvider for CastThenFinish {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let finish = request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::ChooseResolution);
        self.declinable.push(finish.is_some());
        let cast = request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::CastSpell);
        if self.casts > 0
            && let Some(cast) = cast
        {
            self.casts -= 1;
            return Ok(cast);
        }
        finish
            .or(cast)
            .ok_or_else(|| EngineError::new("the request offers neither a cast nor a finish"))
    }
}

/// Takes the first offered option of every request.
struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.options.is_empty() {
            return Err(EngineError::new(format!("{} offers no option", request.id)));
        }
        Ok(0)
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: id.to_string(),
        power: type_line.contains("Creature").then(|| "1".to_string()),
        rules: Vec::new(),
        toughness: type_line.contains("Creature").then(|| "1".to_string()),
        type_line: type_line.to_string(),
    }
}

/// An instant or sorcery whose resolution gains its controller `life`.
fn life_spell(id: &str, type_line: &str, life: i64) -> CardDefinition {
    let mut spell = definition(id, type_line);
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": life },
        }],
    })];
    spell
}

fn engine(seed: u64) -> GameEngine {
    let mut filler = definition("cast-filler", "Basic Land - Island");
    filler.power = None;
    filler.toughness = None;
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
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
        seed,
    )
    .expect("cast test setup is valid");
    engine.state.step = GameStep::PrecombatMain;
    engine
}

fn instance(id: &str, definition: CardDefinition) -> CardInstance {
    let mut card = card_instance("player-0", &definition, 0);
    card.instance_id = id.to_string();
    card
}

fn source_stack(engine: &GameEngine) -> StackObject {
    StackObject {
        id: "stack:frame-source".to_string(),
        controller: "player-0".to_string(),
        card: card_instance(
            &engine.state.players[0].id,
            &definition("frame-source", "Sorcery"),
            0,
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// The resolution-time cast the cast frame lowers for a bound card.
fn resolution_cast(binding: &str, zone: &str, without_paying: bool) -> Value {
    json!({
        "kind": "castAnyNumber",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "cards": { "kind": "boundObjects", "binding": binding },
        "where": { "kind": "canBeCastAsSpell" },
        "timing": { "kind": "duringResolution" },
        "withoutPayingManaCost": without_paying,
        "alternativeCostsAllowed": !without_paying,
        "additionalCostsApply": true,
        "variableManaValue": { "kind": "integer", "value": 0 },
        "sourceZone": zone,
        "maximum": { "kind": "integer", "value": 1 },
        "bind": "castCards",
    })
}

fn bound(id: &str) -> BTreeMap<String, RuntimeBinding> {
    BTreeMap::from([(
        "picked".to_string(),
        RuntimeBinding::Objects(vec![id.to_string()]),
    )])
}

#[test]
fn an_imperative_cast_offers_no_decline_while_the_optional_cast_offers_one() {
    for (mandatory, expect_declinable) in [(false, true), (true, false)] {
        let mut engine = engine(41);
        engine.state.players[0].graveyard.push(instance(
            "bound-instant",
            life_spell("bound-instant", "Instant", 3),
        ));
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let mut cast = resolution_cast("picked", "graveyard", false);
        if mandatory {
            cast["mandatory"] = json!(true);
        }
        assert!(effect_supported(&cast), "{cast}");
        let stack = source_stack(&engine);
        let mut provider = CastThenFinish {
            casts: 1,
            declinable: Vec::new(),
        };
        engine
            .execute_effects(
                &json!([cast]),
                &stack,
                &mut bound("bound-instant"),
                &mut BTreeMap::new(),
                &mut provider,
            )
            .expect("the bound card is cast at its printed cost");
        assert_eq!(provider.declinable, [expect_declinable]);
        assert_eq!(engine.state.stack.len(), 1, "the bound card is the spell");
        assert_eq!(engine.state.stack[0].card.instance_id, "bound-instant");
    }
}

#[test]
fn the_cast_instant_is_exiled_instead_of_put_into_its_owners_graveyard() {
    let mut engine = engine(43);
    engine.state.players[0].graveyard.push(instance(
        "echo-instant",
        life_spell("echo-instant", "Instant", 2),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut cast = resolution_cast("picked", "graveyard", true);
    cast["exileInsteadOfGraveyard"] = json!({ "graveyard": "any" });
    assert!(effect_supported(&cast), "{cast}");
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bound("echo-instant"),
            &mut BTreeMap::new(),
            &mut CastThenFinish {
                casts: 1,
                declinable: Vec::new(),
            },
        )
        .expect("the instant is cast without paying");
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("the cast instant resolves");
    assert_eq!(engine.state.players[0].life, 22);
    let player = &engine.state.players[0];
    assert!(
        player
            .exile
            .iter()
            .any(|card| card.instance_id == "echo-instant"),
        "the replacement exiles the resolved instant"
    );
    assert!(
        !player
            .graveyard
            .iter()
            .any(|card| card.instance_id == "echo-instant")
    );
}

#[test]
fn a_your_graveyard_replacement_does_not_apply_to_an_opponents_spell() {
    let mut engine = engine(47);
    let mut stolen = card_instance("player-1", &life_spell("stolen-instant", "Instant", 1), 0);
    stolen.instance_id = "stolen-instant".to_string();
    engine.state.players[1].graveyard.push(stolen);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut cast = resolution_cast("picked", "graveyard", true);
    cast["exileInsteadOfGraveyard"] = json!({ "graveyard": "controller" });
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bound("stolen-instant"),
            &mut BTreeMap::new(),
            &mut CastThenFinish {
                casts: 1,
                declinable: Vec::new(),
            },
        )
        .expect("an opponent's card is cast from their graveyard");
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("the cast instant resolves");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "stolen-instant"),
        "`your graveyard` does not watch the owner's other graveyard"
    );
}

#[test]
fn three_card_copies_are_cast_individually_and_the_uncast_one_ceases_to_exist() {
    let mut engine = engine(53);
    engine.state.players[0].exile.push(instance(
        "deluge-target",
        life_spell("deluge-target", "Sorcery", 4),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let copy = |bind: &str| {
        json!({
            "kind": "createCardCopy",
            "card": { "kind": "boundObject", "binding": "exiledObject" },
            "fromZone": "exile",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "to": "exile",
            "bind": bind,
        })
    };
    let copies = json!({ "kind": "union", "sets": [
        { "kind": "boundObjects", "binding": "cardCopy" },
        { "kind": "boundObjects", "binding": "cardCopy2" },
        { "kind": "boundObjects", "binding": "cardCopy3" },
    ]});
    let mut cast = resolution_cast("unused", "exile", true);
    cast["cards"] = copies.clone();
    cast["maximum"] = json!({ "kind": "integer", "value": 3 });
    let effects = json!([
        copy("cardCopy"),
        copy("cardCopy2"),
        copy("cardCopy3"),
        cast,
        { "kind": "ceaseToExist", "objects": copies, "fromZone": "exile" },
    ]);
    for effect in effects.as_array().unwrap() {
        assert!(effect_supported(effect), "{effect}");
    }
    let stack = source_stack(&engine);
    let mut bindings = BTreeMap::from([(
        "exiledObject".to_string(),
        RuntimeBinding::Objects(vec!["deluge-target".to_string()]),
    )]);
    let mut provider = CastThenFinish {
        casts: 2,
        declinable: Vec::new(),
    };
    engine
        .execute_effects(
            &effects,
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("the copies are created and two of them cast");
    assert_eq!(provider.declinable, [true, true, true]);
    assert_eq!(engine.state.stack.len(), 2, "two copies are spells");
    assert!(
        engine
            .state
            .stack
            .iter()
            .all(|spell| is_spell_copy(&spell.card))
    );
    let exile = &engine.state.players[0].exile;
    assert_eq!(exile.len(), 1, "the uncast copy ceased to exist");
    assert_eq!(exile[0].instance_id, "deluge-target");
    for _ in 0..2 {
        engine
            .resolve_top_stack(&mut FirstOption)
            .expect("a cast copy resolves");
    }
    assert_eq!(engine.state.players[0].life, 28);
    assert_eq!(engine.state.players[0].exile.len(), 1);
    assert!(engine.state.players[0].graveyard.is_empty());
}

#[test]
fn a_card_owned_outside_the_game_is_chosen_revealed_and_put_into_hand() {
    let mut engine = engine(59);
    let mut wished = card_instance("player-0", &definition("wished-sorcery", "Sorcery"), 0);
    wished.instance_id = "wished-sorcery".to_string();
    let mut other = card_instance("player-0", &definition("sideboard-creature", "Creature"), 0);
    other.instance_id = "sideboard-creature".to_string();
    engine.state.players[0].sideboard = vec![wished, other];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let effects = json!([
        {
            "kind": "chooseCards",
            "id": "chosenHiddenCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "candidates": {
                "kind": "cards",
                "zone": { "kind": "outsideGame", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
                "where": { "kind": "cardTypeContains", "value": "Sorcery" },
            },
            "minimum": { "kind": "integer", "value": 1 },
            "maximum": { "kind": "integer", "value": 1 },
        },
        { "kind": "revealCards", "cards": { "kind": "decisionResult", "decisionId": "chosenHiddenCards" } },
        {
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": "chosenHiddenCards" },
            "to": { "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        },
    ]);
    for effect in effects.as_array().unwrap() {
        assert!(effect_supported(effect), "{effect}");
    }
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &effects,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the wish resolves");
    let player = &engine.state.players[0];
    assert!(
        player
            .hand
            .iter()
            .any(|card| card.instance_id == "wished-sorcery"),
        "the chosen sorcery enters the game into its owner's hand"
    );
    assert_eq!(player.sideboard.len(), 1);
    assert_eq!(player.sideboard[0].instance_id, "sideboard-creature");
}

#[test]
fn a_card_chosen_at_random_from_a_hand_is_replayed_by_its_seed() {
    let run = |seed: u64| {
        let mut engine = engine(seed);
        engine.state.players[0].hand = (0..5)
            .map(|index| {
                let id = format!("hand-instant-{index}");
                instance(&id, definition(&id, "Instant"))
            })
            .collect();
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let effects = json!([
            {
                "kind": "chooseCards",
                "id": "chosenHiddenCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "candidates": {
                    "kind": "cards",
                    "zone": { "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
                    "where": { "kind": "cardTypeContains", "value": "Instant" },
                },
                "minimum": { "kind": "integer", "value": 1 },
                "maximum": { "kind": "integer", "value": 1 },
                "selection": { "kind": "random" },
            },
            { "kind": "revealCards", "cards": { "kind": "decisionResult", "decisionId": "chosenHiddenCards" } },
        ]);
        for effect in effects.as_array().unwrap() {
            assert!(effect_supported(effect), "{effect}");
        }
        let stack = source_stack(&engine);
        let mut decisions = BTreeMap::new();
        engine
            .execute_effects(
                &effects,
                &stack,
                &mut BTreeMap::new(),
                &mut decisions,
                &mut FirstOption,
            )
            .expect("the random reveal asks the player nothing");
        decisions["chosenHiddenCards"].clone()
    };
    let first = run(61);
    assert_eq!(first.as_array().map(Vec::len), Some(1));
    assert_eq!(first, run(61), "the same seed replays the same card");
}

/// Takes the first option of the given action kind, else the first option.
struct PreferKind(ActionKind);

impl DecisionProvider for PreferKind {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.options.is_empty() {
            return Err(EngineError::new(format!("{} offers no option", request.id)));
        }
        Ok(request
            .options
            .iter()
            .position(|action| action.kind == self.0)
            .unwrap_or(0))
    }
}

fn costed(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    let mut card = life_spell(id, type_line, 1);
    card.mana_cost = mana_cost.to_string();
    card
}

fn life_cast(binding: &str, zone: &str) -> Value {
    let mut cast = resolution_cast(binding, zone, false);
    cast["alternativeCostsAllowed"] = json!(false);
    cast["alternativeCost"] =
        json!({ "kind": "payLife", "amount": { "kind": "manaValueOfCastCard" } });
    cast
}

#[test]
fn a_life_alternative_cost_pays_the_cast_cards_mana_value_in_life() {
    let mut engine = engine(67);
    engine.state.players[0].hand.push(instance(
        "life-sorcery",
        costed("life-sorcery", "Sorcery", "{2}{B}"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = life_cast("picked", "hand");
    assert!(effect_supported(&cast), "{cast}");
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bound("life-sorcery"),
            &mut BTreeMap::new(),
            &mut CastThenFinish {
                casts: 1,
                declinable: Vec::new(),
            },
        )
        .expect("the sorcery is cast by paying life");
    assert_eq!(engine.state.stack.len(), 1, "no mana source is needed");
    assert_eq!(engine.state.stack[0].card.instance_id, "life-sorcery");
    assert_eq!(
        engine.state.players[0].life, 17,
        "life equal to mana value 3"
    );
}

#[test]
fn a_life_alternative_cost_above_the_life_total_offers_no_cast() {
    let mut engine = engine(71);
    engine.state.players[0].life = 2;
    engine.state.players[0].hand.push(instance(
        "dear-sorcery",
        costed("dear-sorcery", "Sorcery", "{2}{B}"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = life_cast("picked", "hand");
    let stack = source_stack(&engine);
    let mut provider = CastThenFinish {
        casts: 1,
        declinable: Vec::new(),
    };
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bound("dear-sorcery"),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("nothing is castable");
    assert!(provider.declinable.is_empty(), "no decision is offered");
    assert!(engine.state.stack.is_empty());
    assert_eq!(
        engine.state.players[0].life, 2,
        "CR 119.4: no partial payment"
    );
}

#[test]
fn a_two_zone_origin_offers_the_cards_of_both_zones() {
    let mut engine = engine(73);
    engine.state.players[0].hand.push(instance(
        "hand-instant",
        life_spell("hand-instant", "Instant", 1),
    ));
    engine.state.players[0].graveyard.push(instance(
        "yard-instant",
        life_spell("yard-instant", "Instant", 1),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut cast = resolution_cast("picked", "hand", true);
    cast.as_object_mut()
        .expect("the cast is an object")
        .remove("sourceZone");
    cast["sourceZones"] = json!(["hand", "graveyard"]);
    cast["maximum"] = json!({ "kind": "integer", "value": 2 });
    assert!(effect_supported(&cast), "{cast}");
    let stack = source_stack(&engine);
    let mut bindings = BTreeMap::from([(
        "picked".to_string(),
        RuntimeBinding::Objects(vec!["hand-instant".to_string(), "yard-instant".to_string()]),
    )]);
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut CastThenFinish {
                casts: 2,
                declinable: Vec::new(),
            },
        )
        .expect("both cards are cast from their own zones");
    let cast_ids = engine
        .state
        .stack
        .iter()
        .map(|spell| spell.card.instance_id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        cast_ids,
        BTreeSet::from(["hand-instant".to_string(), "yard-instant".to_string()])
    );
}

#[test]
fn a_resolution_play_puts_a_land_onto_the_battlefield_as_the_land_play() {
    let mut engine = engine(79);
    let mut land = definition("exiled-land", "Land");
    land.power = None;
    land.toughness = None;
    engine.state.players[0]
        .exile
        .push(instance("exiled-land", land));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut play = resolution_cast("picked", "exile", true);
    play["playLands"] = json!(true);
    assert!(effect_supported(&play), "{play}");
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([play]),
            &stack,
            &mut bound("exiled-land"),
            &mut BTreeMap::new(),
            &mut PreferKind(ActionKind::PlayLand),
        )
        .expect("the land is played while the effect resolves");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|permanent| permanent.instance_id == "exiled-land")
    );
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "landPlayed"),
        "CR 305.2: the play is a land play of the turn"
    );
}

#[test]
fn your_commander_is_cast_from_the_command_zone_paying_its_tax() {
    for (previous_casts, expect_cast) in [(0, true), (1, false)] {
        let mut engine = engine(83);
        let mut commander = definition("free-commander", "Legendary Creature - Golem");
        commander.is_commander = true;
        commander.mana_cost = "{5}".to_string();
        engine.state.players[0]
            .command_zone
            .push(instance("free-commander", commander));
        engine.state.commanders.push(CommanderState {
            card_instance_id: "free-commander".to_string(),
            name: "free-commander".to_string(),
            owner_id: "player-0".to_string(),
            casts_from_command_zone: previous_casts,
        });
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let mut cast = resolution_cast("unused", "commandZone", true);
        cast.as_object_mut()
            .expect("the cast is an object")
            .remove("cards");
        cast["cardsWhere"] = json!({ "kind": "isCommander" });
        assert!(effect_supported(&cast), "{cast}");
        let stack = source_stack(&engine);
        engine
            .execute_effects(
                &json!([cast]),
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut CastThenFinish {
                    casts: 1,
                    declinable: Vec::new(),
                },
            )
            .expect("the command-zone cast resolves");
        // CR 903.8: without mana sources, only an untaxed commander is cast.
        assert_eq!(!engine.state.stack.is_empty(), expect_cast);
        if expect_cast {
            assert_eq!(engine.state.commanders[0].casts_from_command_zone, 1);
        }
    }
}

#[test]
fn a_next_upkeep_window_ends_when_the_controllers_next_turn_begins() {
    let mut engine = engine(89);
    let mut land = definition("window-land", "Land");
    land.power = None;
    land.toughness = None;
    engine.state.players[0]
        .exile
        .push(instance("window-land", land));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let grant = json!({
        "kind": "grantPermission",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "action": {
            "kind": "play",
            "card": { "kind": "boundObject", "binding": "picked" },
            "normalTimingApplies": true,
            "normalCostsApply": true,
        },
        "duration": {
            "kind": "untilNextUpkeep",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
    });
    assert!(effect_supported(&grant), "{grant}");
    let stack = source_stack(&engine);
    let turn = engine.state.turn_number;
    engine
        .execute_effects(
            &json!([grant]),
            &stack,
            &mut bound("window-land"),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the permission is granted");
    let permission = engine
        .state
        .permissions
        .iter()
        .find(|permission| permission["cardInstanceId"] == "window-land")
        .cloned()
        .expect("a permission names the exiled land");
    // Two players: the controller's next turn is two turns away.
    assert_eq!(permission["expiresAfterTurn"], json!(turn + 1));
    engine.state.turn_number = turn + 1;
    assert!(engine.card_permission_in_scope(&permission, "player-0", None));
    engine.state.turn_number = turn + 2;
    assert!(
        !engine.card_permission_in_scope(&permission, "player-0", None),
        "the window is over at the controller's next upkeep"
    );
}

fn attacking_move(policy: Value) -> Value {
    json!({
        "kind": "moveCards",
        "cards": { "kind": "boundObjects", "binding": "picked" },
        "to": {
            "kind": "battlefield",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "tapped": true,
            "attacking": policy,
        },
    })
}

#[test]
fn a_permanent_put_onto_the_battlefield_attacking_was_never_declared() {
    let mut engine = engine(97);
    engine.state.step = GameStep::DeclareBlockers;
    engine.state.players[0].hand.push(instance(
        "entering-attacker",
        definition("entering-attacker", "Creature - Soldier"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let effect = attacking_move(json!({ "kind": "anyDefender" }));
    assert!(effect_supported(&effect), "{effect}");
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([effect]),
            &stack,
            &mut bound("entering-attacker"),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the creature enters tapped and attacking");
    let permanent = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "entering-attacker")
        .expect("the creature is on the battlefield");
    assert!(permanent.tapped);
    assert_eq!(permanent.flags.get("attacking"), Some(&true));
    let attacker = engine
        .state
        .combat
        .attackers
        .iter()
        .find(|attacker| attacker.attacker_id == "entering-attacker")
        .expect("the creature is an attacking creature");
    assert_eq!(attacker.defending_player_id, "player-1");
    // CR 508.4: it was never declared, so nothing sees a declaration.
    assert!(!engine.state.events.iter().any(|event| {
        event.kind == "attackerDeclared"
            && event.card_instance_id.as_deref() == Some("entering-attacker")
    }));
}

#[test]
fn an_attacking_entry_naming_an_unattackable_player_enters_without_attacking() {
    let mut engine = engine(101);
    engine.state.step = GameStep::DeclareBlockers;
    engine.state.players[0].hand.push(instance(
        "misdirected-attacker",
        definition("misdirected-attacker", "Creature - Soldier"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    // The named player is the controller, whom it cannot attack (CR 506.2).
    let effect = attacking_move(json!({
        "kind": "player",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
    }));
    assert!(effect_supported(&effect), "{effect}");
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([effect]),
            &stack,
            &mut bound("misdirected-attacker"),
            &mut BTreeMap::new(),
            &mut FirstOption,
        )
        .expect("the creature enters tapped");
    let permanent = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "misdirected-attacker")
        .expect("the creature is on the battlefield");
    assert!(permanent.tapped);
    assert_eq!(permanent.flags.get("attacking"), None);
    assert!(engine.state.combat.attackers.is_empty());
}
