//! Keyword rows unlocked by shared grammar (batch 7), run from real Oracle
//! keyword lines through the playable-card adapter that flattens the
//! expansion: station (CR 702.184a, 722.2b) with its tapped-creature cost
//! object, embalm and eternalize (CR 702.128a, 702.129a) with their copy
//! exception lists, and scavenge (CR 702.97a) reading the card it exiled.
use super::*;

fn compiled(
    name: &str,
    type_line: &str,
    power_toughness: (&str, &str),
    mana_cost: &str,
    oracle: &str,
) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace([' ', ','], "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some(power_toughness.0.to_string()),
        toughness: Some(power_toughness.1.to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some(mana_cost.to_string()),
            oracle_text: Some(oracle.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{name}: {:#?}",
        compilation.parser_result.diagnostics
    );
    compilation.card
}

fn creature(id: &str, power: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Test");
    definition.power = Some(power.to_string());
    definition.toughness = Some("2".to_string());
    let mut instance = test_instance(id, definition, "player-0");
    instance.summoning_sick = false;
    instance
}

fn permanent<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    &engine.state.players[player].battlefield[index]
}

fn activations(engine: &GameEngine, source: &str) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some(source)
        })
        .collect()
}

fn activate_and_resolve(engine: &mut GameEngine, action: &LegalAction) {
    engine
        .apply_priority_action(action, &mut RandomAi::seeded(7))
        .expect("the activation pays its costs");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the ability resolves");
}

const BORG_SCOUT_SPHERE: &str = "When this Spacecraft enters, draw a card.\nStation (Tap another creature you control: Put charge counters equal to its power on this Spacecraft. Station only as a sorcery. It's an artifact creature at 6+.)\n6+ | Artifact creatures you control have flying.";

/// The station activation tapping `creature` (the cost's chosen object).
fn station_tapping(engine: &GameEngine, creature: &str) -> Option<LegalAction> {
    activations(engine, "sphere").into_iter().find(|action| {
        matches!(
            action.targets.get("activationCostTap0"),
            Some(TargetRef::Permanent { instance_id }) if instance_id == creature
        )
    })
}

fn charge_counters(engine: &GameEngine, id: &str) -> i32 {
    permanent(engine, id)
        .counters
        .get("charge")
        .copied()
        .unwrap_or(0)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn station_charges_by_the_tapped_creature_power_until_its_creature_band_opens() {
    let sphere = compiled(
        "Borg Scout Sphere",
        "Artifact \u{2014} Spacecraft",
        ("5", "5"),
        "{2}{U}",
        BORG_SCOUT_SPHERE,
    );
    let station = sphere
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("activatedAbility"))
        .expect("the station activation");
    assert_eq!(station["keywordProvenance"]["keyword"], "station");
    assert_eq!(station["costs"][0]["bindPowerAs"], "tappedPermanentPower");
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        test_instance("sphere", sphere, "player-0"),
        creature("ogre", "4"),
        creature("bear", "3"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(!engine.permanent_is_creature(permanent(&engine, "sphere")));
    // CR 702.184a: only another creature pays the cost, never the source.
    assert!(station_tapping(&engine, "sphere").is_none());

    let ogre = station_tapping(&engine, "ogre").expect("station tapping the ogre");
    activate_and_resolve(&mut engine, &ogre);
    assert!(permanent(&engine, "ogre").tapped);
    assert!(!permanent(&engine, "sphere").tapped);
    assert_eq!(charge_counters(&engine, "sphere"), 4);
    // Below the 6+ striation the Spacecraft is not a creature (CR 722.2b).
    assert!(!engine.permanent_is_creature(permanent(&engine, "sphere")));
    // A tapped creature cannot pay the cost again.
    assert!(station_tapping(&engine, "ogre").is_none());

    let bear = station_tapping(&engine, "bear").expect("station tapping the bear");
    activate_and_resolve(&mut engine, &bear);
    assert_eq!(charge_counters(&engine, "sphere"), 7);
    let sphere = permanent(&engine, "sphere");
    assert!(engine.permanent_is_creature(sphere));
    assert_eq!(
        (
            engine.current_power(sphere),
            engine.current_toughness(sphere)
        ),
        (5, 5)
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn station_is_offered_only_at_sorcery_speed() {
    let sphere = compiled(
        "Borg Scout Sphere",
        "Artifact \u{2014} Spacecraft",
        ("5", "5"),
        "{2}{U}",
        BORG_SCOUT_SPHERE,
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        test_instance("sphere", sphere, "player-0"),
        creature("ogre", "4"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(station_tapping(&engine, "ogre").is_some());
    // Another player's priority in the same main phase is not sorcery
    // timing for player 0 (CR 307.1).
    engine.state.active_player = 1;
    assert!(station_tapping(&engine, "ogre").is_none());
}

fn token<'a>(engine: &'a GameEngine) -> &'a CardInstance {
    engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.definition.is_token)
        .expect("the created token copy")
}

fn graveyard_engine(
    card: CardInstance,
    lands: &[(&str, &str, &str)],
    others: Vec<CardInstance>,
) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = lands
        .iter()
        .map(|(id, type_line, mana)| mana_source(id, type_line, mana, None))
        .chain(others)
        .collect();
    engine.state.players[0].graveyard.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn embalm_creates_a_white_zombie_copy_with_no_mana_cost() {
    let cat = compiled(
        "Sacred Cat",
        "Creature \u{2014} Cat",
        ("1", "1"),
        "{W}",
        "Lifelink\nEmbalm {W} ({W}, Exile this card from your graveyard: Create a token that's a copy of it, except it's a white Zombie Cat with no mana cost. Embalm only as a sorcery.)",
    );
    let mut engine = graveyard_engine(
        test_instance("sacred-cat", cat, "player-0"),
        &[("plains", "Basic Land - Plains", "{W}")],
        Vec::new(),
    );
    let action = activations(&engine, "sacred-cat")
        .into_iter()
        .next()
        .expect("embalm is offered from the graveyard");
    activate_and_resolve(&mut engine, &action);
    assert!(engine.state.players[0].graveyard.is_empty());
    let embalmed = token(&engine);
    // CR 702.128a: white, no mana cost, a Zombie in addition to its types.
    assert_eq!(
        engine.permanent_color_symbols(embalmed),
        BTreeSet::from(["W".to_string()])
    );
    assert!(embalmed.definition.mana_cost.is_empty());
    assert!(embalmed.definition.type_line.contains("Zombie"));
    assert!(embalmed.definition.type_line.contains("Cat"));
    assert_eq!(
        (
            engine.current_power(embalmed),
            engine.current_toughness(embalmed)
        ),
        (1, 1)
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn eternalize_creates_a_black_four_four_zombie_copy_with_no_mana_cost() {
    let champion = compiled(
        "Champion of Wits",
        "Creature \u{2014} Snake Wizard",
        ("2", "1"),
        "{2}{U}",
        "Eternalize {5}{U}{U} ({5}{U}{U}, Exile this card from your graveyard: Create a token that's a copy of it, except it's a 4/4 black Zombie Snake Wizard with no mana cost. Eternalize only as a sorcery.)",
    );
    let islands = [
        ("island-a", "Basic Land - Island", "{U}"),
        ("island-b", "Basic Land - Island", "{U}"),
        ("island-c", "Basic Land - Island", "{U}"),
        ("island-d", "Basic Land - Island", "{U}"),
        ("island-e", "Basic Land - Island", "{U}"),
        ("island-f", "Basic Land - Island", "{U}"),
        ("island-g", "Basic Land - Island", "{U}"),
    ];
    let mut engine = graveyard_engine(
        test_instance("champion-of-wits", champion, "player-0"),
        &islands,
        Vec::new(),
    );
    let action = activations(&engine, "champion-of-wits")
        .into_iter()
        .next()
        .expect("eternalize is offered from the graveyard");
    activate_and_resolve(&mut engine, &action);
    let eternalized = token(&engine);
    // CR 702.129a: black, 4/4, no mana cost, a Zombie in addition.
    assert_eq!(
        engine.permanent_color_symbols(eternalized),
        BTreeSet::from(["B".to_string()])
    );
    assert!(eternalized.definition.mana_cost.is_empty());
    assert!(eternalized.definition.type_line.contains("Zombie"));
    assert!(eternalized.definition.type_line.contains("Snake"));
    assert_eq!(
        (
            engine.current_power(eternalized),
            engine.current_toughness(eternalized)
        ),
        (4, 4)
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn scavenge_counts_the_power_of_the_card_it_exiled() {
    let goliath = compiled(
        "Deadbridge Goliath",
        "Creature \u{2014} Insect",
        ("5", "5"),
        "{2}{G}{G}",
        "Scavenge {4}{G}{G} ({4}{G}{G}, Exile this card from your graveyard: Put a number of +1/+1 counters equal to this card's power on target creature. Scavenge only as a sorcery.)",
    );
    let forests = [
        ("forest-a", "Basic Land - Forest", "{G}"),
        ("forest-b", "Basic Land - Forest", "{G}"),
        ("forest-c", "Basic Land - Forest", "{G}"),
        ("forest-d", "Basic Land - Forest", "{G}"),
        ("forest-e", "Basic Land - Forest", "{G}"),
        ("forest-f", "Basic Land - Forest", "{G}"),
    ];
    let mut engine = graveyard_engine(
        test_instance("deadbridge-goliath", goliath, "player-0"),
        &forests,
        vec![creature("bear", "2")],
    );
    let action = activations(&engine, "deadbridge-goliath")
        .into_iter()
        .next()
        .expect("scavenge is offered from the graveyard");
    activate_and_resolve(&mut engine, &action);
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "deadbridge-goliath")
    );
    // CR 702.97a: the counters equal the exiled card's power.
    assert_eq!(permanent(&engine, "bear").counters.get("+1/+1"), Some(&5));
}
