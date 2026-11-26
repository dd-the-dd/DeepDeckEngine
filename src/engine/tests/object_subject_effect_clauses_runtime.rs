//! Real Oracle text -> primitive IR -> engine behavior for object-subject
//! effect clauses: an inline resolution choice in the subject (`creatures of
//! the creature type of your choice get +2/+2`, CR 608.2d) and the subject
//! shapes the clause layer reads for grants and restriction riders.
use super::*;

/// Picks the resolution option whose single value is the requested word.
struct ChooseOption(&'static str);

impl DecisionProvider for ChooseOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        request
            .options
            .iter()
            .position(|action| {
                action.decisions.values().any(|value| {
                    value.as_str() == Some(self.0)
                        || value.as_array().is_some_and(|values| {
                            values.first().and_then(Value::as_str) == Some(self.0)
                        })
                })
            })
            .ok_or_else(|| EngineError::new("the requested resolution option is offered"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line, "power": "2", "toughness": "2",
    }))
    .expect("standalone test definition is valid")
}

fn permanent(id: &str, type_line: &str, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition(id, type_line), 0);
    instance.instance_id = id.into();
    instance
}

fn engine_with_tribes() -> GameEngine {
    let filler = definition("filler", "Basic Land — Forest");
    let mut engine = GameEngine::new(
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
        1208,
    )
    .expect("standalone test engine is valid");
    for (id, type_line) in [
        ("elf", "Creature — Elf Warrior"),
        ("goblin", "Creature — Goblin"),
    ] {
        engine.state.players[0]
            .battlefield
            .push(permanent(id, type_line, "player-0"));
    }
    // The opponent's Elf is of the chosen type too: the subject names no
    // controller.
    engine.state.players[1]
        .battlefield
        .push(permanent("rival-elf", "Creature — Elf", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture permanents are admitted");
    engine
}

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("object-subject-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status == "canonical" && ability.rule.is_some())
        .unwrap_or_else(|| panic!("a canonical rule: {parsed:#?}"));
    let rule = ability.rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn resolve(engine: &mut GameEngine, rule: &Value, chosen: &'static str) {
    let stack_object = StackObject {
        id: "stack:inline-choice".to_string(),
        controller: "player-0".to_string(),
        card: permanent("inline-choice-source", "Sorcery", "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut ChooseOption(chosen),
        )
        .expect("the parsed program resolves");
}

fn power_of(engine: &GameEngine, id: &str) -> i32 {
    let card = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("the permanent is on the battlefield");
    engine.current_power(card)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_inline_type_choice_pumps_only_the_chosen_type() {
    // Tribal Forcemage's effect, as a spell: the controller chooses on
    // resolution, then only creatures of that type get the bonus. The pump
    // lasts after the choice's decisions are gone (CR 611.2a).
    let rule = parsed_rule(
        "Tribal Forcemage Effect",
        "Sorcery",
        "Creatures of the creature type of your choice get +2/+2 and gain trample until end of turn.",
    );
    assert_eq!(rule["effects"][0]["kind"], "chooseCreatureType");
    let mut engine = engine_with_tribes();
    resolve(&mut engine, &rule, "Elf");
    assert_eq!(power_of(&engine, "elf"), 4);
    assert_eq!(power_of(&engine, "rival-elf"), 4);
    assert_eq!(power_of(&engine, "goblin"), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_inline_type_choice_feeds_a_one_shot_collection() {
    // Extinction: the destruction reads the type chosen first.
    let rule = parsed_rule(
        "Extinction",
        "Sorcery",
        "Destroy all creatures of the creature type of your choice.",
    );
    let mut engine = engine_with_tribes();
    resolve(&mut engine, &rule, "Goblin");
    let survivors: Vec<_> = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .map(|card| card.instance_id.clone())
        .collect();
    assert!(survivors.contains(&"elf".to_string()), "{survivors:?}");
    assert!(
        survivors.contains(&"rival-elf".to_string()),
        "{survivors:?}"
    );
    assert!(!survivors.contains(&"goblin".to_string()), "{survivors:?}");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_entry_sentence_makes_the_copied_token_enter_tapped_and_attacking() {
    // Thousand-Faced Shadow's shape without its intervening condition: the
    // token a copy creation makes enters tapped and attacking (CR 508.4).
    let rule = parsed_rule(
        "Entry Sentence Probe",
        "Sorcery",
        "Create a token that's a copy of target creature you control. The token enters tapped and attacking.",
    );
    let mut engine = test_engine(2);
    let attacker = test_instance(
        "ninja",
        test_definition("ninja", "Creature - Human Ninja"),
        "player-0",
    );
    engine.state.players[0].battlefield = vec![attacker];
    // A permanent enters attacking only during combat, under the active
    // player's control (CR 508.4).
    engine.state.active_player = 0;
    engine.state.step = GameStep::DeclareBlockers;
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "ninja".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("the copied creature is a declared target")
        .to_string();
    let stack_object = StackObject {
        id: "stack:entry-sentence".to_string(),
        controller: "player-0".to_string(),
        card: permanent("entry-source", "Sorcery", "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "ninja".to_string(),
            },
        )]),
    };
    let mut decisions = BTreeMap::new();
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut EmeritusDecisionProvider,
        )
        .expect("the copy creation resolves");
    let tokens = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|card| card.definition.is_token)
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].tapped);
    assert_eq!(tokens[0].flags.get("attacking"), Some(&true));
    assert_eq!(engine.state.combat.attackers.len(), 2);
}

fn compile_free(name: &str, text: &str) -> CardDefinition {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let compiled = std::thread::Builder::new()
        .name("object-subject-compiler".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            compile_playable_card(PlayableCardInput {
                id: "distributive-source".to_string(),
                face_id: None,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                power: None,
                toughness: None,
                oracle: request,
            })
        })
        .expect("compiler thread starts")
        .join()
        .expect("compiler thread completes")
        .expect("the card compiles");
    assert_eq!(compiled.engine_status, "executable", "{name}");
    compiled.card
}

fn sized_creature(id: &str, power: &str, toughness: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Bear");
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    test_instance(id, definition, controller)
}

/// Casts the compiled sorcery for free, declaring exactly `wanted` through
/// the legal cast actions, then resolves it.
fn cast_and_resolve(engine: &mut GameEngine, card: &CardDefinition, wanted: &[&str]) {
    engine.state.players[0].hand.push(test_instance(
        "distributive-source",
        card.clone(),
        "player-0",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let wanted = wanted
        .iter()
        .map(|id| id.to_string())
        .collect::<BTreeSet<_>>();
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action
                    .targets
                    .values()
                    .filter_map(|target| match target {
                        TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>()
                    == wanted
        })
        .unwrap_or_else(|| panic!("no cast declaring exactly {wanted:?}"));
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell is cast: {}", error.message));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell resolves: {}", error.message));
}

fn damage_on(engine: &GameEngine, id: &str) -> i64 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .map_or(-1, |card| i64::from(card.damage_marked))
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn each_declared_member_deals_damage_equal_to_its_own_power() {
    // Allies at Last: each of the two declared creatures deals damage equal
    // to its own power to the one declared recipient (CR 608.2c).
    let card = compile_free(
        "Allies at Last",
        "Up to two target creatures you control each deal damage equal to their power to target creature an opponent controls.",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        sized_creature("small", "2", "2", "player-0"),
        sized_creature("large", "3", "3", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![sized_creature("wall", "0", "9", "player-1")];
    cast_and_resolve(&mut engine, &card, &["small", "large", "wall"]);
    assert_eq!(damage_on(&engine, "wall"), 5);
}
