//! Runtime proof of the keyword-action rows of batch 8 (`keyword_action.rs`,
//! `keyword_action_programs.rs`): the Oracle text is parsed, its lowered
//! program executes, and the resulting game state is checked. Incubate
//! (CR 701.53), face status (CR 708), unattach (CR 701.3), Room doors
//! (CR 709.5f-g), cloak (CR 701.58) and fateseal (CR 701.29).
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

fn stack_source(card: CardInstance, controller: &str, kind: &str) -> StackObject {
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: controller.to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(kind.to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn spell_source(name: &str, type_line: &str) -> StackObject {
    stack_source(
        test_instance(name, test_definition(name, type_line), "player-0"),
        "player-0",
        "spellAbility",
    )
}

fn targeting(mut source: StackObject, id: &str, instance_id: &str) -> StackObject {
    source.targets.insert(
        id.to_string(),
        TargetRef::Permanent {
            instance_id: instance_id.to_string(),
        },
    );
    source
}

fn resolve(engine: &mut GameEngine, effects: &Value, source: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        effects,
        source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the program resolves");
}

fn run(engine: &mut GameEngine, effect: Value, source: &StackObject) {
    assert!(effect_supported(&effect), "{effect}");
    resolve(engine, &Value::Array(vec![effect]), source);
}

fn event_count(engine: &GameEngine, kind: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == kind)
        .count()
}

fn find_permanent<'a>(engine: &'a GameEngine, player: usize, name: &str) -> &'a CardInstance {
    engine.state.players[player]
        .battlefield
        .iter()
        .find(|permanent| permanent.definition.name == name)
        .unwrap_or_else(|| panic!("{name} is on the battlefield"))
}

#[test]
fn incubate_creates_an_incubator_that_transforms_into_a_phyrexian_with_its_counters() {
    let rules = parsed_rules(
        "Incubate Witness",
        "Sorcery",
        "Incubate 3. (Create an Incubator token with three +1/+1 counters on it and \"{2}: Transform this token.\" It transforms into a 0/0 Phyrexian artifact creature.)",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &spell_source("incubate", "Sorcery"),
    );
    let incubator = find_permanent(&engine, 0, "Incubator Token").clone();
    assert!(incubator.definition.is_token);
    assert!(type_line_contains(
        &incubator.definition.type_line,
        "Artifact"
    ));
    assert!(!type_line_contains(
        &incubator.definition.type_line,
        "Creature"
    ));
    assert_eq!(incubator.counters.get("+1/+1").copied(), Some(3));
    // The front face has "{2}: Transform this token." (CR 701.53b).
    let activated = incubator
        .definition
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("activatedAbility"))
        .expect("the Incubator has its transform ability");
    assert!(rule_is_executable(activated), "{activated:#}");
    // Transforming keeps the object and its counters: a 0/0 Phyrexian
    // artifact creature with three +1/+1 counters is a 3/3.
    let phyrexian = engine
        .transform_permanent(&incubator.instance_id, None)
        .expect("the Incubator transforms");
    assert_eq!(phyrexian.instance_id, incubator.instance_id);
    assert_eq!(phyrexian.definition.name, "Phyrexian Token");
    assert!(type_line_contains(
        &phyrexian.definition.type_line,
        "Creature"
    ));
    assert_eq!(current_power(&phyrexian), 3);
    assert!(
        phyrexian
            .definition
            .rules
            .iter()
            .all(|rule| value_kind(rule) != Some("activatedAbility")),
        "the back face has no transform ability"
    );
}

#[test]
fn incubate_twice_creates_two_tokens_and_incubate_zero_still_creates_one() {
    let rules = parsed_rules("Incubate Witness", "Sorcery", "Incubate 2 twice.");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &spell_source("incubate-twice", "Sorcery"),
    );
    let incubators = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.name == "Incubator Token")
        .collect::<Vec<_>>();
    assert_eq!(incubators.len(), 2);
    assert!(
        incubators
            .iter()
            .all(|token| token.counters.get("+1/+1").copied() == Some(2))
    );

    // CR 701.53a: incubate 0 creates the token with no counters.
    let mut zero = rules[0]["effects"][0]["effects"][0].clone();
    zero["counters"][0]["count"] = json!({ "kind": "integer", "value": 0 });
    let mut empty = test_engine(2);
    empty.state.players[0].battlefield.clear();
    run(&mut empty, zero, &spell_source("incubate-zero", "Sorcery"));
    let token = find_permanent(&empty, 0, "Incubator Token");
    assert_eq!(token.counters.get("+1/+1").copied().unwrap_or(0), 0);
}

fn morph_creature(id: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Bear");
    definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "morph",
            "cost": { "kind": "payMana", "manaCost": "{1}" },
            "putCounterWhenTurnedFaceUp": false,
        },
    }));
    test_instance(id, definition, "player-1")
}

#[test]
fn turning_a_permanent_face_down_then_up_keeps_the_object_and_its_counters() {
    let rules = parsed_rules("Face Witness", "Instant", "Turn target creature face down.");
    let mut engine = test_engine(2);
    let mut bear = morph_creature("bear");
    bear.counters.insert("+1/+1".to_string(), 1);
    bear.tapped = true;
    engine.state.players[1].battlefield = vec![bear];
    let target_id = rules[0]["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("one declared target")
        .to_string();
    let source = targeting(spell_source("face-down", "Instant"), &target_id, "bear");
    resolve(&mut engine, &rules[0]["effects"], &source);
    let face_down = engine.state.players[1].battlefield[0].clone();
    assert_eq!(face_down.instance_id, "bear");
    assert_eq!(face_down.definition.name, "Face-down creature");
    assert_eq!(face_down.flags.get("faceDown"), Some(&true));
    assert_eq!(face_down.counters.get("+1/+1").copied(), Some(1));
    assert!(face_down.tapped, "CR 708.8: the same object stays tapped");
    assert_eq!(current_power(&face_down), 3);
    assert_eq!(event_count(&engine, "permanentTurnedFaceDown"), 1);

    // CR 708.2b: a face-down permanent can't be turned face down again.
    resolve(&mut engine, &rules[0]["effects"], &source);
    assert_eq!(
        engine.state.players[1].battlefield[0].definition,
        face_down.definition
    );
    assert_eq!(event_count(&engine, "permanentTurnedFaceDown"), 1);

    // CR 708.8: turned face up, its printed characteristics return.
    let face_up = json!({
        "kind": "turnPermanentFaceUp",
        "permanent": { "kind": "chosenTarget", "id": target_id },
    });
    run(&mut engine, face_up, &source);
    let restored = &engine.state.players[1].battlefield[0];
    assert_eq!(restored.definition.name, "bear");
    assert_eq!(restored.flags.get("faceDown"), None);
    assert_eq!(restored.counters.get("+1/+1").copied(), Some(1));
    assert_eq!(event_count(&engine, "permanentTurnedFaceUp"), 1);
}

#[test]
fn double_faced_permanents_stay_face_up_and_face_down_spells_stay_face_down() {
    let mut engine = test_engine(2);
    // CR 712.13: a transforming double-faced permanent isn't turned face down.
    let mut double_faced = test_definition("werewolf", "Creature - Werewolf");
    double_faced.rules.push(json!({
        "kind": "rulesMarker",
        "text": "Transformable card faces.",
        "transformFaces": [
            { "name": "werewolf", "typeLine": "Creature - Werewolf", "manaCost": "{0}" },
            { "name": "beast", "typeLine": "Creature - Werewolf", "manaCost": "" },
        ],
    }));
    let werewolf = test_instance("werewolf", double_faced, "player-0");
    // CR 701.40g: a manifested instant card turned face up stays face down.
    let mut manifested = test_instance(
        "manifested",
        test_definition("manifested", "Instant"),
        "player-0",
    );
    manifested.definition = face_down_creature_definition(&manifested.definition);
    manifested.flags.insert("faceDown".to_string(), true);
    engine.state.players[0].battlefield = vec![werewolf, manifested];
    let source = spell_source("face-status", "Instant");
    run(
        &mut engine,
        json!({ "kind": "turnPermanentFaceDown", "permanent": { "kind": "eachPermanent" } }),
        &source,
    );
    let werewolf = find_permanent(&engine, 0, "werewolf");
    assert_eq!(werewolf.flags.get("faceDown"), None);
    run(
        &mut engine,
        json!({ "kind": "turnPermanentFaceUp", "permanent": { "kind": "eachPermanent" } }),
        &source,
    );
    let manifested = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "manifested")
        .expect("the manifested card stays");
    assert_eq!(manifested.flags.get("faceDown"), Some(&true));
    assert_eq!(event_count(&engine, "permanentTurnedFaceUp"), 0);
    assert_eq!(event_count(&engine, "faceDownPermanentRevealed"), 1);
}

#[test]
fn unattach_leaves_the_equipment_on_the_battlefield_attached_to_nothing() {
    let rules = parsed_rules(
        "Unattach Witness",
        "Artifact - Equipment",
        "{1}: Unattach this Equipment.",
    );
    let mut engine = test_engine(2);
    let bear = test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    );
    let mut sword = test_instance(
        "sword",
        test_definition("sword", "Artifact - Equipment"),
        "player-0",
    );
    sword.attached_to = Some("bear".to_string());
    engine.state.players[0].battlefield = vec![bear, sword.clone()];
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &stack_source(sword, "player-0", "activatedAbility"),
    );
    let sword = find_permanent(&engine, 0, "sword");
    assert_eq!(sword.attached_to, None);
    assert_eq!(event_count(&engine, "permanentUnattached"), 1);
    // Unattaching what is attached to nothing does nothing.
    let source = stack_source(sword.clone(), "player-0", "activatedAbility");
    resolve(&mut engine, &rules[0]["effects"], &source);
    assert_eq!(event_count(&engine, "permanentUnattached"), 1);
}

fn room(id: &str) -> CardInstance {
    let mut definition = test_definition(id, "Enchantment - Room // Enchantment - Room");
    definition.mana_cost = "{1}{R} // {2}{R}".to_string();
    test_instance(id, definition, "player-0")
}

#[test]
fn room_doors_unlock_and_lock_by_instruction() {
    let mut engine = test_engine(2);
    let mut lounge = room("lounge");
    lounge.flags.insert("roomDoor0Unlocked".to_string(), true);
    engine.state.players[0].battlefield = vec![lounge];
    let source = targeting(spell_source("doors", "Instant"), "targetRoom", "lounge");
    let change = |change: &str| {
        json!({
            "kind": "changeRoomDoor",
            "permanent": { "kind": "chosenTarget", "id": "targetRoom" },
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "change": change,
        })
    };
    // The only locked door is unlocked (CR 709.5f).
    run(&mut engine, change("unlock"), &source);
    let doors = |engine: &GameEngine| {
        (0..2)
            .map(|door| {
                engine.state.players[0].battlefield[0]
                    .flags
                    .get(&format!("roomDoor{door}Unlocked"))
                    .copied()
                    == Some(true)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(doors(&engine), vec![true, true]);
    assert_eq!(event_count(&engine, "doorUnlocked"), 1);
    // A fully unlocked Room has no locked door left to unlock.
    run(&mut engine, change("unlock"), &source);
    assert_eq!(event_count(&engine, "doorUnlocked"), 1);
    // CR 709.5g: one chosen unlocked door is locked.
    run(&mut engine, change("lockOrUnlock"), &source);
    assert_eq!(
        doors(&engine).iter().filter(|unlocked| **unlocked).count(),
        1
    );
    assert_eq!(event_count(&engine, "doorLocked"), 1);
}

#[test]
fn cloak_puts_the_top_card_onto_the_battlefield_face_down_with_ward() {
    let rules = parsed_rules(
        "Cloak Witness",
        "Sorcery",
        "Cloak the top card of your library.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    let giant = test_instance(
        "giant",
        test_definition("giant", "Creature - Giant"),
        "player-0",
    );
    // The top of a library is its last card.
    engine.state.players[0].library.push(giant);
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &spell_source("cloak", "Sorcery"),
    );
    let cloaked = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "giant")
        .expect("the top card is cloaked");
    assert_eq!(cloaked.flags.get("faceDown"), Some(&true));
    assert_eq!(cloaked.definition.name, "Face-down creature");
    assert!(cloaked.definition.rules.iter().any(|rule| {
        value_kind(rule) == Some("keywordAbility") && value_kind(&rule["ability"]) == Some("ward")
    }));
    assert_eq!(
        cloaked
            .printed_definition
            .as_ref()
            .map(|printed| printed.name.as_str()),
        Some("giant")
    );
}

#[test]
fn fateseal_reorders_an_opponents_library_without_changing_its_cards() {
    let rules = parsed_rules("Fateseal Witness", "Sorcery", "Fateseal 2.");
    let mut engine = test_engine(2);
    let before = engine.state.players[1]
        .library
        .iter()
        .map(|card| card.instance_id.clone())
        .collect::<BTreeSet<_>>();
    let own_before = engine.state.players[0].library.len();
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &spell_source("fateseal", "Sorcery"),
    );
    let after = engine.state.players[1]
        .library
        .iter()
        .map(|card| card.instance_id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(before, after, "the cards stay in that library");
    assert_eq!(engine.state.players[0].library.len(), own_before);
}
