//! Runtime proofs of the batch-8 cast frame primitives: a resolution-time
//! cast of library cards the effect observed (CR 608.2g, 601.2a), the
//! graveyard exile a granted casting permission carries to its spells (CR
//! 614.1a), and the cards a graveyard-arrival record names while each is
//! still the object the event saw (CR 400.7, 603.2c).
use super::super::super::*;

/// Casts the first offered card once, then finishes.
struct CastOnce {
    cast: bool,
}

impl DecisionProvider for CastOnce {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let cast = request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::CastSpell);
        let finish = request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::ChooseResolution);
        if !self.cast
            && let Some(cast) = cast
        {
            self.cast = true;
            return Ok(cast);
        }
        finish
            .or(cast)
            .ok_or_else(|| EngineError::new("the request offers neither a cast nor a finish"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{1}".to_string(),
        name: id.to_string(),
        power: None,
        rules: vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "gainLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 2 },
            }],
        })],
        toughness: None,
        type_line: type_line.to_string(),
    }
}

fn engine(seed: u64) -> GameEngine {
    let mut filler = definition("library-filler", "Basic Land - Island");
    filler.rules = Vec::new();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 8],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        seed,
    )
    .expect("cast query test setup is valid");
    engine.state.step = GameStep::PrecombatMain;
    engine
}

fn instance(owner: &str, id: &str, definition: CardDefinition) -> CardInstance {
    let mut card = card_instance(owner, &definition, 0);
    card.instance_id = id.to_string();
    card
}

fn source_stack(engine: &GameEngine) -> StackObject {
    StackObject {
        id: "stack:cast-query-source".to_string(),
        controller: "player-0".to_string(),
        card: card_instance(
            &engine.state.players[0].id,
            &definition("cast-query-source", "Sorcery"),
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

/// `Look at the top four cards of your library. You may cast a spell from
/// among them without paying its mana cost.`: the observed card below the
/// top is cast from where it is; an unobserved library card is not offered.
#[test]
fn an_observed_library_card_is_cast_from_its_position() {
    let mut engine = engine(81);
    let library = &mut engine.state.players[0].library;
    let below_top = library.len().saturating_sub(1);
    library.insert(
        below_top,
        instance(
            "player-0",
            "observed-sorcery",
            definition("observed-sorcery", "Sorcery"),
        ),
    );
    library.insert(
        0,
        instance(
            "player-0",
            "unobserved-sorcery",
            definition("unobserved-sorcery", "Sorcery"),
        ),
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = json!({
        "kind": "castAnyNumber",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "cards": { "kind": "boundObjects", "binding": "lookedCards" },
        "where": { "kind": "canBeCastAsSpell" },
        "timing": { "kind": "duringResolution" },
        "withoutPayingManaCost": true,
        "alternativeCostsAllowed": false,
        "additionalCostsApply": true,
        "variableManaValue": { "kind": "integer", "value": 0 },
        "sourceZone": "library",
        "maximum": { "kind": "integer", "value": 1 },
        "bind": "castCards",
    });
    assert!(effect_supported(&cast), "{cast}");
    let stack = source_stack(&engine);
    let mut bindings = BTreeMap::from([(
        "lookedCards".to_string(),
        RuntimeBinding::Objects(vec!["observed-sorcery".to_string()]),
    )]);
    engine
        .execute_effects(
            &json!([cast]),
            &stack,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut CastOnce { cast: false },
        )
        .expect("the observed library card is cast without paying");
    assert_eq!(engine.state.stack.len(), 1, "one spell was cast");
    assert_eq!(engine.state.stack[0].card.instance_id, "observed-sorcery");
    assert!(
        engine.state.players[0]
            .library
            .iter()
            .any(|card| card.instance_id == "unobserved-sorcery"),
        "a library card the effect did not observe stays where it is"
    );
}

/// The library-position admission is a decision of the resolution-time
/// offering only; a hand action is unchanged.
#[test]
fn only_a_library_action_is_admitted_at_any_position() {
    let mut library_action = LegalAction {
        id: "cast:library".to_string(),
        kind: ActionKind::CastSpell,
        player_id: "player-0".to_string(),
        label: String::new(),
        card_instance_id: Some("card".to_string()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::from([("castSourceZone".to_string(), json!("library"))]),
        targets: BTreeMap::new(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    };
    let mut hand_action = library_action.clone();
    hand_action
        .decisions
        .insert("castSourceZone".to_string(), json!("hand"));
    resolution_casting::admit_observed_library_position(&mut library_action);
    resolution_casting::admit_observed_library_position(&mut hand_action);
    assert!(resolution_casting::observed_library_position_admitted(
        &library_action.decisions
    ));
    assert!(!resolution_casting::observed_library_position_admitted(
        &hand_action.decisions
    ));
}

/// `You may cast it this turn. If that spell would be put into your
/// graveyard, exile it instead.`: the granted permission carries the
/// replacement, the cast carries it to the spell, and the instant cast that
/// way has its graveyard destination replaced.
#[test]
fn a_granted_permission_carries_the_graveyard_exile_to_its_spell() {
    let mut engine = engine(83);
    engine.state.players[0].exile.push(instance(
        "player-0",
        "permitted-instant",
        definition("permitted-instant", "Instant"),
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let grant = json!({
        "kind": "grantPermission",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "action": {
            "kind": "cast",
            "card": { "kind": "boundObject", "binding": "exiledObject" },
            "normalTimingApplies": true,
            "normalCostsApply": true,
            "exileInsteadOfGraveyard": { "graveyard": "controller" },
        },
        "duration": { "kind": "untilEndOfCurrentTurn" },
    });
    assert!(effect_supported(&grant), "{grant}");
    let mut refused = grant.clone();
    refused["action"]["kind"] = json!("play");
    assert!(
        !effect_supported(&refused),
        "a play permission casts no spell to replace"
    );
    let stack = source_stack(&engine);
    engine
        .execute_effects(
            &json!([grant]),
            &stack,
            &mut BTreeMap::from([(
                "exiledObject".to_string(),
                RuntimeBinding::Objects(vec!["permitted-instant".to_string()]),
            )]),
            &mut BTreeMap::new(),
            &mut CastOnce { cast: true },
        )
        .expect("the permission is granted");
    let permission = engine
        .state
        .permissions
        .iter()
        .find(|permission| permission["cardInstanceId"] == "permitted-instant")
        .cloned()
        .expect("the permission names the exiled card");
    assert_eq!(
        permission["exileInsteadOfGraveyard"]["graveyard"],
        "controller"
    );
    let mut decisions = BTreeMap::new();
    GameEngine::carry_permission_graveyard_exile(&permission, &mut decisions);
    // The spell is on the stack as the cast put it there.
    let card = engine.state.players[0].exile.remove(0);
    let mut spell = source_stack(&engine);
    spell.id = "stack:permitted-instant".to_string();
    spell.card = card;
    spell.ability_kind = None;
    engine.state.stack.push(spell);
    engine.install_permitted_cast_graveyard_exile(&decisions, "permitted-instant");
    assert!(
        engine.state.rule_modifiers.iter().any(|modifier| {
            modifier["kind"] == "exileSpecificCardInsteadOfGraveyard"
                && modifier["cardInstanceId"] == "permitted-instant"
        }),
        "the cast instant carries the replacement of its graveyard destination"
    );
}

/// `Whenever one or more land cards are put into your graveyard from your
/// library, put them onto the battlefield tapped.`: the record names every
/// card of the event while it is still that object in the graveyard.
#[test]
fn a_graveyard_arrival_names_its_cards_while_they_are_those_objects() {
    let mut engine = engine(85);
    for id in ["milled-land-a", "milled-land-b"] {
        engine.state.players[0]
            .graveyard
            .push(instance("player-0", id, definition(id, "Land")));
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut decisions = BTreeMap::from([(
        "triggeringCardIds".to_string(),
        json!(["milled-land-a", "milled-land-b"]),
    )]);
    engine.capture_event_object(&mut decisions);
    let mut stack = source_stack(&engine);
    stack.decisions = decisions;
    assert_eq!(
        engine.event_card_ids(&stack),
        ["milled-land-a", "milled-land-b"]
    );
    // One card left the graveyard: it is a new object, no longer named.
    let moved = engine.state.players[0].graveyard.remove(0);
    engine.state.players[0].exile.push(moved);
    assert_eq!(engine.event_card_ids(&stack), ["milled-land-b"]);
}
