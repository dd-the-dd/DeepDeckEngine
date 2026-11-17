//! Real Oracle text -> primitive IR -> engine behavior for two shared frames:
//! the static player-permission frame (actor + play/cast/look action + origin
//! + manner) and the choose-then-move selection frame (`return N PERMANENTS
//! you control to its owner's hand`).
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn parsed_card(
    instance_id: &str,
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle: &str,
) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.mana_cost = mana_cost.to_string();
    definition.rules = parsed_rules(name, type_line, mana_cost, oracle);
    test_instance(instance_id, definition, "player-0")
}

fn card(instance_id: &str, type_line: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.mana_cost = String::new();
    test_instance(instance_id, definition, "player-0")
}

fn has_action(engine: &GameEngine, kind: ActionKind, card_id: &str, zone: &str) -> bool {
    engine.legal_priority_actions(0).into_iter().any(|action| {
        action.kind == kind
            && action.card_instance_id.as_deref() == Some(card_id)
            && action
                .decisions
                .get("castSourceZone")
                .and_then(Value::as_str)
                == Some(zone)
    })
}

#[test]
fn parsed_graveyard_land_permission_plays_only_land_cards_from_graveyard() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![parsed_card(
        "explorer",
        "Icetill Explorer",
        "Creature — Insect Scout",
        "{2}{G}{G}",
        "You may play an additional land on each of your turns.\nYou may play lands from your graveyard.\nLandfall — Whenever a land you control enters, mill a card.",
    )];
    engine.state.players[0].graveyard = vec![
        card("graveyard-land", "Land — Forest"),
        card("graveyard-spell", "Sorcery"),
    ];
    assert!(!has_action(
        &engine,
        ActionKind::CastSpell,
        "graveyard-spell",
        "graveyard"
    ));
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::PlayLand
                && action.card_instance_id.as_deref() == Some("graveyard-land")
                && action.decisions["castSourceZone"] == "graveyard"
        })
        .expect("the parsed permission makes the graveyard land playable");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("the graveyard land is played");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "graveyard-land")
    );

    // Without the permission source the same graveyard land is not playable.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].graveyard = vec![card("graveyard-land", "Land — Forest")];
    assert!(!has_action(
        &engine,
        ActionKind::PlayLand,
        "graveyard-land",
        "graveyard"
    ));
}

#[test]
fn parsed_flash_permission_covers_only_the_first_matching_spell_each_turn() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![parsed_card(
        "spirit",
        "The Blue Spirit",
        "Legendary Creature — Human Rogue Ally",
        "{2}{U}",
        "You may cast the first creature spell you cast each turn as though it had flash.",
    )];
    let creature = test_definition("bear", "Creature — Bear");
    let sorcery = test_definition("ritual", "Sorcery");
    assert!(engine.card_casts_with_static_flash("player-0", &creature, "hand"));
    assert!(!engine.card_casts_with_static_flash("player-0", &sorcery, "hand"));
    assert!(!engine.player_casts_with_flash("player-0"));
    // An opponent's creature spell is not this player's first creature spell.
    engine.record_event(
        "spellCast",
        Some("player-1".to_string()),
        Some("opponent-bear".to_string()),
        json!({"typeLine": "Creature — Bear"}),
    );
    assert!(engine.card_casts_with_static_flash("player-0", &creature, "hand"));
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("first-bear".to_string()),
        json!({"typeLine": "Creature — Bear"}),
    );
    assert!(!engine.card_casts_with_static_flash("player-0", &creature, "hand"));

    // The same frame without the ordinal grants flash to every matching spell.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![parsed_card(
        "leyline",
        "Leyline of Anticipation",
        "Enchantment",
        "{2}{U}{U}",
        "If this card is in your opening hand, you may begin the game with it on the battlefield.\nYou may cast spells as though they had flash.",
    )];
    assert!(engine.player_casts_with_flash("player-0"));
    assert!(!engine.player_casts_with_flash("player-1"));
}

#[test]
fn parsed_library_top_permission_casts_matching_spells_and_plays_lands_only_when_named() {
    let chorus = || {
        parsed_card(
            "chorus",
            "Elven Chorus",
            "Enchantment",
            "{3}{G}",
            "You may look at the top card of your library any time.\nYou may cast creature spells from the top of your library.\nCreatures you control have \"{T}: Add one mana of any color.\"",
        )
    };
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![chorus()];
    engine.state.players[0].library = vec![card("top-creature", "Creature — Elf")];
    assert!(has_action(
        &engine,
        ActionKind::CastSpell,
        "top-creature",
        "library"
    ));
    // A land is neither cast nor played through a cast permission.
    engine.state.players[0].library = vec![card("top-land", "Land — Forest")];
    assert!(!has_action(
        &engine,
        ActionKind::PlayLand,
        "top-land",
        "library"
    ));

    // Unused combination: play lands + cast a subtype-filtered spell query.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![parsed_card(
        "chocobo",
        "Probe Chocobo",
        "Creature — Bird",
        "{2}{G}",
        "You may play lands and cast Bird spells from the top of your library.",
    )];
    engine.state.players[0].library = vec![card("top-land", "Land — Forest")];
    assert!(has_action(
        &engine,
        ActionKind::PlayLand,
        "top-land",
        "library"
    ));
    engine.state.players[0].library = vec![card("top-elf", "Creature — Elf")];
    assert!(!has_action(
        &engine,
        ActionKind::CastSpell,
        "top-elf",
        "library"
    ));
}

fn resolve_entry_trigger(engine: &mut GameEngine, karoo: CardInstance) {
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![karoo];
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::PlayLand
                && action.card_instance_id.as_deref() == Some("karoo")
        })
        .expect("the karoo land play is legal");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("the karoo is played");
    assert_eq!(engine.state.stack.len(), 1, "the entry trigger waits");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the selection and move resolve");
}

fn dimir_aqueduct() -> CardInstance {
    parsed_card(
        "karoo",
        "Dimir Aqueduct",
        "Land",
        "",
        "This land enters tapped.\nWhen this land enters, return a land you control to its owner's hand.\n{T}: Add {U}{B}.",
    )
}

#[test]
fn parsed_karoo_returns_exactly_one_chosen_controlled_land_to_its_owners_hand() {
    let mut engine = test_engine(2);
    engine.state.players[1].hand.clear();
    let mut borrowed = card("borrowed-land", "Land — Island");
    borrowed.owner = "player-1".to_string();
    engine.state.players[0].battlefield = vec![card("own-land", "Land — Swamp"), borrowed];
    engine.state.players[1].battlefield = vec![card("opponent-land", "Land — Island")];
    engine.state.players[1].battlefield[0].owner = "player-1".to_string();
    engine.state.players[1].battlefield[0].controller = "player-1".to_string();
    resolve_entry_trigger(&mut engine, dimir_aqueduct());
    let returned = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.hand.iter().map(|card| card.instance_id.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        returned.len(),
        1,
        "exactly one land is chosen: {returned:?}"
    );
    assert_ne!(returned[0], "opponent-land", "only a controlled land");
    let owner_hand = if returned[0] == "borrowed-land" { 1 } else { 0 };
    assert_eq!(engine.state.players[owner_hand].hand.len(), 1);
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "opponent-land")
    );
}

#[test]
fn parsed_upkeep_selection_returns_only_matching_permanents_and_tolerates_none() {
    let advisor_rule = || {
        parsed_rules(
            "Trusted Advisor",
            "Creature — Human Advisor",
            "{U}",
            "At the beginning of your upkeep, return a blue creature you control to its owner's hand.",
        )
        .into_iter()
        .next()
        .expect("one triggered rule")
    };
    let mut blue = card("blue-creature", "Creature — Merfolk");
    blue.definition.mana_cost = "{U}".to_string();
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].battlefield = vec![card("green-creature", "Creature — Elf"), blue];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = engine.state.players[0].battlefield[0].clone();
    engine.put_triggered_ability_on_stack_with_decisions(&source, advisor_rule(), BTreeMap::new());
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the selection resolves");
    let hand = engine.state.players[0]
        .hand
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(hand, ["blue-creature"]);

    // No matching permanent: the instruction does as much as possible (CR 609.3).
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].battlefield = vec![card("green-creature", "Creature — Elf")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = engine.state.players[0].battlefield[0].clone();
    engine.put_triggered_ability_on_stack_with_decisions(&source, advisor_rule(), BTreeMap::new());
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("an empty exact selection resolves without moving anything");
    assert!(engine.state.players[0].hand.is_empty());
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
}
