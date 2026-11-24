//! Runtime proofs of the object-query theme and its PR 75 review
//! follow-ups: main-phase ordinals from the turn's step history, Room doors
//! off the battlefield, two scry modes of one modal spell, and the history
//! predicates of the shared object query.
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
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
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn spell_source(id: &str, controller: &str, decisions: BTreeMap<String, Value>) -> StackObject {
    StackObject {
        id: format!("stack:{id}"),
        controller: controller.to_string(),
        card: test_instance(id, test_definition(id, "Instant"), controller),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    }
}

#[test]
fn the_main_phase_ordinal_is_read_from_the_turn_step_history() {
    use super::super::phase_ordinal::{
        main_phase_ordinal_in, main_phase_ordinal_of_step, turn_step_history,
    };
    // CR 505.1a: the precombat main phase is the first main phase of the
    // turn and the postcombat one the second, whatever combat ran between.
    for combats in [0, 1, 3] {
        let history = turn_step_history(combats);
        assert_eq!(main_phase_ordinal_in(&history, "precombatMain"), Some(1));
        assert_eq!(main_phase_ordinal_in(&history, "postcombatMain"), Some(2));
        assert_eq!(main_phase_ordinal_in(&history, "upkeep"), None);
    }
    // The validation reader and the runtime reader share that history.
    assert_eq!(main_phase_ordinal_of_step("postcombatMain"), Some(2));
    // A history without a postcombat main phase has no second main phase.
    let truncated = [GameStep::Upkeep, GameStep::PrecombatMain];
    assert_eq!(main_phase_ordinal_in(&truncated, "postcombatMain"), None);
}

#[test]
fn room_doors_are_all_unlocked_off_the_battlefield() {
    // CR 709.3/709.5: a Room's doors are battlefield designations. Off the
    // battlefield the card has both halves' abilities; on the battlefield a
    // door's abilities exist only once that door is unlocked.
    let mut definition = test_definition("room", "Enchantment \u{2014} Room");
    definition.rules = vec![
        json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "flying" },
            "roomDoorIndex": 0,
        }),
        json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "vigilance" },
            "roomDoorIndex": 1,
        }),
    ];
    assert!(has_keyword(&definition, "flying"));
    assert!(has_keyword(&definition, "vigilance"));
    let mut room = test_instance("room", definition, "player-0");
    assert!(!permanent_printed_has_keyword(&room, "flying"));
    assert!(!permanent_printed_has_keyword(&room, "vigilance"));
    room.flags.insert("roomDoor1Unlocked".to_string(), true);
    assert!(!permanent_printed_has_keyword(&room, "flying"));
    assert!(permanent_printed_has_keyword(&room, "vigilance"));
}

#[test]
fn a_modal_spell_resolves_two_chosen_scry_modes_in_order() {
    // Kozilek's Command shape: two chosen modes both look at the library.
    // Each mode keeps its own scry; neither replaces the other.
    let rules = parsed_rules(
        "Two Scry Modes",
        "Instant",
        "Choose two \u{2014}\n\u{2022} Scry 1.\n\u{2022} Scry 2.\n\u{2022} You gain 3 life.",
    );
    let spell = rules
        .iter()
        .find(|rule| rule["kind"] == "spellAbility")
        .expect("the modal spell ability compiles");
    let modes = spell["declaration"]["decisions"]
        .as_array()
        .and_then(|decisions| {
            decisions
                .iter()
                .find(|decision| decision["kind"] == "chooseModes")
        })
        .expect("a mode declaration");
    let decision_id = modes["id"].as_str().expect("mode decision id").to_string();
    let options = modes["options"].as_array().expect("mode options");
    let chosen = json!([options[0].clone(), options[1].clone()]);
    let mut engine = test_engine(2);
    engine.state.players[0].library = (0..4)
        .map(|index| {
            let id = format!("library-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-0")
        })
        .collect();
    let mut decisions = BTreeMap::from([(decision_id, chosen)]);
    let source = spell_source("two-scry-modes", "player-0", decisions.clone());
    execute_effects_at_resolution_start(
        &mut engine,
        &spell["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut decisions,
        &mut EmeritusDecisionProvider,
    )
    .expect("both chosen modes resolve");
    let scries = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "scried")
        .map(|event| event.detail["count"].clone())
        .collect::<Vec<_>>();
    assert_eq!(scries, vec![json!(1), json!(2)]);
    assert_eq!(
        engine.state.players[0].life, 20,
        "the third mode was not chosen"
    );
    assert_eq!(engine.state.players[0].library.len(), 4);
}

fn creature(id: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Bear");
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, owner)
}

fn damage_to_player(engine: &mut GameEngine, source: &str, player: &str, combat: bool) {
    engine.record_event(
        "damageDealt",
        Some(player.to_string()),
        None,
        json!({ "amount": 2, "combat": combat, "source": source }),
    );
}

#[test]
fn the_damage_recipient_relation_reads_this_turn_damage_to_that_player() {
    // CR 120.1/510.2: `that dealt [combat] damage to PLAYER this turn` reads
    // damage events of this turn whose source is the permanent and whose
    // recipient is that player, relative to the ability controller.
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        creature("raider", "player-1"),
        creature("pinger", "player-1"),
        creature("idle", "player-1"),
    ];
    damage_to_player(&mut engine, "raider", "player-0", true);
    damage_to_player(&mut engine, "pinger", "player-0", false);
    let relation = |recipient: &str, combat_only: bool| json!({ "kind": "dealtDamageToPlayerThisTurn", "recipient": recipient, "combatOnly": combat_only });
    let reads = |engine: &GameEngine, filter: &Value, controller: &str| {
        engine.state.players[1]
            .battlefield
            .iter()
            .filter(|permanent| {
                engine.permanent_dealt_damage_to_player_this_turn(permanent, filter, controller)
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        reads(&engine, &relation("abilityController", false), "player-0"),
        vec!["raider", "pinger"]
    );
    assert_eq!(
        reads(&engine, &relation("abilityController", true), "player-0"),
        vec!["raider"],
        "only combat damage"
    );
    // Relative to an ability controlled by player 1, player 0 is an
    // opponent: the same damage reads as damage to an opponent.
    assert_eq!(
        reads(
            &engine,
            &relation("opponentOfAbilityController", false),
            "player-1"
        ),
        vec!["raider", "pinger"]
    );
    assert!(reads(&engine, &relation("abilityController", false), "player-1").is_empty());
    // An unknown relation never matches every damage event.
    assert!(reads(&engine, &relation("everyone", false), "player-0").is_empty());
    assert!(
        !super::super::event_history::dealt_damage_to_player_supported(&relation(
            "everyone", false
        ))
    );
    // A previous turn's damage is not this turn's history.
    engine.state.turn_number += 1;
    assert!(reads(&engine, &relation("anyPlayer", false), "player-0").is_empty());
}

#[test]
fn destroying_all_creatures_that_dealt_damage_to_you_reads_the_resolving_controller() {
    let rules = parsed_rules(
        "History Probe",
        "Sorcery",
        "Destroy all creatures that dealt damage to you this turn.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield =
        vec![creature("raider", "player-1"), creature("idle", "player-1")];
    damage_to_player(&mut engine, "raider", "player-0", true);
    let source = spell_source("history-probe", "player-0", BTreeMap::new());
    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the collection resolves");
    let survivors = engine.state.players[1]
        .battlefield
        .iter()
        .map(|permanent| permanent.instance_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(survivors, vec!["idle"]);
}

#[test]
fn card_types_among_a_filtered_graveyard_collection_count_only_its_cards() {
    // `for each card type among creature cards in your graveyard`: an
    // artifact creature and a creature give two types; the instant and the
    // land are outside the collection.
    let rules = parsed_rules(
        "Measure Probe",
        "Sorcery",
        "Draw a card for each card type among creature cards in your graveyard.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard = [
        ("bear", "Creature \u{2014} Bear"),
        ("golem", "Artifact Creature \u{2014} Golem"),
        ("bolt", "Instant"),
        ("land", "Land"),
    ]
    .into_iter()
    .map(|(id, type_line)| test_instance(id, test_definition(id, type_line), "player-0"))
    .collect();
    engine.state.players[0].library = (0..5)
        .map(|index| {
            let id = format!("library-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-0")
        })
        .collect();
    let hand = engine.state.players[0].hand.len();
    let source = spell_source("measure-probe", "player-0", BTreeMap::new());
    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the measure resolves");
    assert_eq!(engine.state.players[0].hand.len(), hand + 2);
}
