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
