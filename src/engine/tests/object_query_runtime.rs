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
