//! Object-to-zone frame runtime: the resolving spell's own move replaces its
//! move to the graveyard (CR 608.2n), `shuffle X into a library` shuffles each
//! library that received a card (CR 701.24b), and a move over a population of
//! graveyards exiles each card in its owner's exile (CR 400.3, 406.2).
use super::*;

fn owner_of_self() -> Value {
    json!({"kind":"ownerOf", "object":{"kind":"self"}})
}

fn each_owner() -> Value {
    json!({"kind":"ownerOf", "object":{"kind":"candidate"}})
}

fn spell(instance_id: &str, effects: Value) -> CardInstance {
    let mut definition = test_definition(instance_id, "Sorcery");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": {"kind": "self"},
        "effects": effects,
    })];
    for rule in &definition.rules {
        assert!(rule_is_executable(rule), "{rule:#}");
    }
    test_instance(instance_id, definition, "player-0")
}

fn cast_and_resolve(engine: &mut GameEngine, instance_id: &str) {
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some(instance_id)
        })
        .expect("the spell can be cast from hand");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("the spell is cast");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell resolves");
}

fn zone_has(cards: &[CardInstance], instance_id: &str) -> bool {
    cards.iter().any(|card| card.instance_id == instance_id)
}

/// `Exile NAME.` as the last instruction of a spell: the spell's own move
/// to exile replaces its move to the graveyard as it finishes resolving.
#[test]
fn a_spell_that_exiles_itself_ends_in_exile_not_in_its_graveyard() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![spell(
        "recollection",
        json!([{
            "kind": "moveCards",
            "cards": {"kind": "self"},
            "to": {"kind": "exile", "player": owner_of_self(), "faceDown": false},
        }]),
    )];
    cast_and_resolve(&mut engine, "recollection");
    assert!(zone_has(&engine.state.players[0].exile, "recollection"));
    assert!(!zone_has(
        &engine.state.players[0].graveyard,
        "recollection"
    ));
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some("exileResolvingSpell")),
        "the finishing step consumes the recorded move"
    );
}

/// `Shuffle NAME into its owner's library.`: the spell ends in its owner's
/// library, and the shuffle of the libraries that received a moved card
/// does not need the spell to be there yet.
#[test]
fn a_spell_that_shuffles_itself_ends_in_its_owners_library() {
    let mut engine = test_engine(2);
    let library_before = engine.state.players[0].library.len();
    engine.state.players[0].hand = vec![spell(
        "zenith",
        json!([
            {
                "kind": "moveCards",
                "cards": {"kind": "self"},
                "to": {"kind": "library", "player": owner_of_self(), "position": "bottom"},
                "bind": "shuffledSource",
            },
            {
                "kind": "shuffleLibrariesHolding",
                "cards": {"kind": "boundObjects", "binding": "shuffledSource"},
            },
        ]),
    )];
    cast_and_resolve(&mut engine, "zenith");
    assert!(zone_has(&engine.state.players[0].library, "zenith"));
    assert_eq!(engine.state.players[0].library.len(), library_before + 1);
    assert!(!zone_has(&engine.state.players[0].graveyard, "zenith"));
}

/// `shuffle it into its owner's library` for another player's card: the
/// card goes to its owner's library, and only that library is shuffled.
#[test]
fn shuffling_a_card_into_a_library_shuffles_only_the_library_that_received_it() {
    let mut engine = test_engine(2);
    engine.state.players[1].graveyard = vec![test_instance(
        "relic",
        test_definition("relic", "Artifact"),
        "player-1",
    )];
    engine.state.players[0].hand = vec![spell(
        "tidy",
        json!([
            {
                "kind": "moveCards",
                "cards": {
                    "kind": "cardsInZone",
                    "zone": {
                        "kind": "graveyard",
                        "players": {
                            "kind": "opponentsOf",
                            "player": {"kind": "controllerOf", "object": {"kind": "self"}},
                        },
                    },
                },
                "to": {"kind": "library", "player": each_owner(), "position": "bottom"},
                "order": {"kind": "random"},
                "bind": "shuffledCards",
            },
            {
                "kind": "shuffleLibrariesHolding",
                "cards": {"kind": "boundObjects", "binding": "shuffledCards"},
            },
        ]),
    )];
    let events_before = engine.state.events.len();
    cast_and_resolve(&mut engine, "tidy");
    assert!(zone_has(&engine.state.players[1].library, "relic"));
    let shuffled = engine.state.events[events_before..]
        .iter()
        .filter(|event| event.kind == "zoneShuffled")
        .map(|event| event.player_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(shuffled, vec![Some("player-1".to_string())]);
}

/// `exile all graveyards`: every card of every graveyard moves to its
/// owner's exile; the spell itself is put into the graveyard afterwards.
#[test]
fn a_population_of_graveyards_is_exiled_card_by_card_to_each_owner() {
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard = vec![test_instance(
        "mine",
        test_definition("mine", "Instant"),
        "player-0",
    )];
    engine.state.players[1].graveyard = vec![test_instance(
        "theirs",
        test_definition("theirs", "Creature - Bear"),
        "player-1",
    )];
    engine.state.players[0].hand = vec![spell(
        "rest",
        json!([{
            "kind": "moveCards",
            "cards": {
                "kind": "cardsInZone",
                "zone": {"kind": "graveyard", "players": {"kind": "eachPlayer"}},
            },
            "to": {"kind": "exile", "player": each_owner(), "faceDown": false},
        }]),
    )];
    cast_and_resolve(&mut engine, "rest");
    assert!(zone_has(&engine.state.players[0].exile, "mine"));
    assert!(zone_has(&engine.state.players[1].exile, "theirs"));
    assert_eq!(
        engine.state.players[0]
            .graveyard
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rest"],
        "only the resolved spell is in a graveyard"
    );
    assert!(engine.state.players[1].graveyard.is_empty());
}

/// `exile all creatures`: the shared permanent exile writer over a
/// resolution-time collection.
#[test]
fn exiling_a_permanent_collection_exiles_every_match_and_nothing_else() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        test_instance(
            "wolf",
            test_definition("wolf", "Creature - Wolf"),
            "player-1",
        ),
        test_instance("rock", test_definition("rock", "Artifact"), "player-1"),
    ];
    engine.state.players[0].hand = vec![spell(
        "judgment",
        json!([{
            "kind": "exilePermanent",
            "permanent": {
                "kind": "eachPermanent",
                "where": {"kind": "cardTypeContains", "value": "Creature"},
            },
        }]),
    )];
    cast_and_resolve(&mut engine, "judgment");
    assert!(zone_has(&engine.state.players[0].exile, "bear"));
    assert!(zone_has(&engine.state.players[1].exile, "wolf"));
    assert!(zone_has(&engine.state.players[1].battlefield, "rock"));
}
