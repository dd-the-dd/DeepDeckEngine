//! Runtime proofs of the engine gaps closed for canonical rules: the
//! initiative (CR 725), created-trigger publication sites (CR 603.7), damage
//! dealt to the source itself, per-member counter counts, source-bound
//! comparison operands, `ownedBy` / `extremumAmong` relations, relation
//! filtered collections, forecast hand-activation limits and the prepared
//! designation. Each real card is parsed from its Oracle text, certified
//! executable, then run; a near miss shows the refusal or the no-op.
use super::super::source_damage_observation::DamageRecipientRef;
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

fn rule_with_effects<'a>(rules: &'a [Value], kind: &str) -> &'a Value {
    rules
        .iter()
        .find(|rule| value_kind(rule) == Some(kind) && rule.get("effects").is_some())
        .expect("the rule kind is present")
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

fn creature(id: &str, owner: &str, power: i32, toughness: i32) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Bear");
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    test_instance(id, definition, owner)
}

fn with_rules(mut card: CardInstance, rules: Vec<Value>) -> CardInstance {
    card.definition.set_printed_rules(rules.clone());
    if let Some(printed) = card.printed_definition.as_mut() {
        printed.set_printed_rules(rules);
    }
    card
}

fn run_effect(engine: &mut GameEngine, effect: &Value, stack_object: &StackObject) {
    assert!(effect_supported(effect), "{effect}");
    engine
        .execute_effect(
            effect,
            None,
            stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the primitive resolves");
}

fn run_program(engine: &mut GameEngine, effects: &Value, stack_object: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        effects,
        stack_object,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the program resolves");
}

fn stack_effect_kinds(engine: &GameEngine) -> Vec<(String, String)> {
    engine
        .state
        .stack
        .iter()
        .map(|object| {
            (
                object.controller.clone(),
                object
                    .ability_rule
                    .as_ref()
                    .and_then(|rule| rule["effects"][0]["kind"].as_str())
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

fn permanent_of<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|permanent| permanent.instance_id == id)
        .expect("the permanent is on the battlefield")
}

// ---------------------------------------------------------------------------
// The initiative (CR 725)
// ---------------------------------------------------------------------------

#[test]
fn taking_the_initiative_ventures_into_undercity_and_combat_damage_takes_it() {
    let rules = parsed_rules(
        "Aarakocra Sneak",
        "Creature \u{2014} Bird Rogue",
        "Flying\nWhen this creature enters, you take the initiative.",
    );
    let take = rule_with_effects(&rules, "triggeredAbility")["effects"][0].clone();
    assert_eq!(value_kind(&take), Some("takeInitiative"));

    let mut engine = test_engine(2);
    let sneak = creature("sneak", "player-0", 1, 4);
    engine.state.players[0].battlefield = vec![sneak.clone()];
    run_effect(
        &mut engine,
        &take,
        &stack_source(sneak.clone(), "player-0", "triggeredAbility"),
    );
    assert_eq!(engine.initiative_holder().as_deref(), Some("player-0"));
    // CR 725.2: taking it triggers the venture into Undercity.
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-0".to_string(), "ventureDungeon".to_string())]
    );
    let venture = engine.state.stack.pop().expect("the venture trigger");
    let venture_effect = venture.ability_rule.as_ref().expect("a rule")["effects"][0].clone();
    assert_eq!(venture_effect["procedure"], "venture into Undercity");
    run_effect(&mut engine, &venture_effect, &venture);
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("dungeonPosition")
            && modifier["playerId"] == "player-0"
            && modifier["room"] == "Secret Entrance"
    }));
    engine.state.stack.clear();

    // CR 725.2: two creatures of one player deal combat damage to the
    // holder in one step: one trigger, controlled by the damaging player.
    let raider = creature("raider", "player-1", 2, 2);
    let second = creature("second-raider", "player-1", 2, 2);
    engine.state.players[1].battlefield = vec![raider, second];
    engine.enqueue_combat_damage_to_player_triggers("raider", "player-0", 2);
    engine.enqueue_combat_damage_to_player_triggers("second-raider", "player-0", 2);
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-1".to_string(), "takeInitiative".to_string())]
    );
    let taken = engine.state.stack.pop().expect("the take trigger");
    let take_effect = taken.ability_rule.as_ref().expect("a rule")["effects"][0].clone();
    run_effect(&mut engine, &take_effect, &taken);
    assert_eq!(engine.initiative_holder().as_deref(), Some("player-1"));
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-1".to_string(), "ventureDungeon".to_string())]
    );
    engine.state.stack.clear();

    // Zero damage, or damage dealt to a player without the initiative,
    // takes nothing.
    engine.enqueue_combat_damage_to_player_triggers("raider", "player-0", 3);
    engine.enqueue_combat_damage_to_player_triggers("raider", "player-1", 0);
    assert!(engine.state.stack.is_empty());

    // CR 725.2: the holder's upkeep ventures; another player's does not.
    engine.state.active_player = 0;
    engine.enqueue_step_triggers("upkeep");
    assert!(engine.state.stack.is_empty());
    engine.state.active_player = 1;
    engine.enqueue_step_triggers("upkeep");
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-1".to_string(), "ventureDungeon".to_string())]
    );
}

#[test]
fn every_initiative_card_of_the_cohort_is_executable_and_a_malformed_player_is_not() {
    for (name, type_line, oracle) in [
        (
            "Explore the Underdark",
            "Sorcery",
            "Search your library for up to two basic land cards and/or Gate cards, put them onto the battlefield tapped, then shuffle.\nYou take the initiative.",
        ),
        (
            "Goliath Paladin",
            "Creature \u{2014} Giant Knight",
            "Vigilance\nWhen this creature enters, you take the initiative.",
        ),
    ] {
        parsed_rules(name, type_line, oracle);
    }
    assert!(!effect_supported(&json!({
        "kind": "takeInitiative",
        "player": { "kind": "nobody" },
    })));
}

// ---------------------------------------------------------------------------
// Created triggers (CR 603.7)
// ---------------------------------------------------------------------------

fn install(engine: &mut GameEngine, effect: &Value, source: &CardInstance, controller: &str) {
    run_effect(
        engine,
        effect,
        &stack_source(source.clone(), controller, "spellAbility"),
    );
}

#[test]
fn a_created_entry_trigger_observes_only_its_controllers_creatures() {
    let rules = parsed_rules(
        "Rite of Harmony",
        "Instant",
        "Whenever a creature or enchantment you control enters this turn, draw a card.\nFlashback {2}{G}{W} (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
    );
    let installer = rule_with_effects(&rules, "spellAbility")["effects"][0].clone();
    assert_eq!(value_kind(&installer), Some("installCreatedTrigger"));
    let mut engine = test_engine(2);
    let rite = test_instance("rite", test_definition("rite", "Instant"), "player-0");
    install(&mut engine, &installer, &rite, "player-0");

    let theirs = creature("their-bear", "player-1", 2, 2);
    engine.state.players[1].battlefield.push(theirs.clone());
    engine.enqueue_enter_battlefield_triggers(&theirs);
    assert!(engine.state.stack.is_empty(), "an opponent's creature");

    let land = test_instance(
        "forest",
        test_definition("forest", "Basic Land \u{2014} Forest"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(land.clone());
    engine.enqueue_enter_battlefield_triggers(&land);
    assert!(
        engine.state.stack.is_empty(),
        "a land is neither creature nor enchantment"
    );

    let ours = creature("our-bear", "player-0", 2, 2);
    engine.state.players[0].battlefield.push(ours.clone());
    engine.enqueue_enter_battlefield_triggers(&ours);
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-0".to_string(), "drawCards".to_string())]
    );
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPermanentId"],
        "our-bear"
    );
}

#[test]
fn a_created_blocker_trigger_names_the_blockers_controller() {
    let rules = parsed_rules(
        "Mage Hunters' Onslaught",
        "Sorcery",
        "Destroy target creature or planeswalker.\nWhenever a creature blocks this turn, its controller loses 1 life.",
    );
    let installer = rule_with_effects(&rules, "spellAbility")["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| value_kind(effect) == Some("installCreatedTrigger"))
        .expect("the created trigger")
        .clone();
    let mut engine = test_engine(2);
    let onslaught = test_instance(
        "onslaught",
        test_definition("onslaught", "Sorcery"),
        "player-0",
    );
    install(&mut engine, &installer, &onslaught, "player-0");
    let attacker = creature("attacker", "player-0", 2, 2);
    let blocker = creature("blocker", "player-1", 2, 2);
    engine.state.players[0].battlefield = vec![attacker];
    engine.state.players[1].battlefield = vec![blocker];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "attacker".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    engine.state.combat.blockers = vec![DeclaredBlocker {
        attacker_id: "attacker".to_string(),
        blocker_id: "blocker".to_string(),
    }];
    let record = engine.block_record("attacker", "blocker", "player-1");
    engine
        .publish_block_observer_triggers(&record)
        .expect("the block is published");
    assert_eq!(
        stack_effect_kinds(&engine),
        vec![("player-0".to_string(), "loseLife".to_string())]
    );
    let trigger = engine.state.stack.pop().expect("the trigger");
    let lose_life = trigger.ability_rule.as_ref().expect("a rule")["effects"][0].clone();
    run_effect(&mut engine, &lose_life, &trigger);
    assert_eq!(engine.state.players[1].life, 19);
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn created_life_gain_and_source_damage_triggers_bind_their_occurrence() {
    let mut engine = test_engine(2);
    let guildmage = creature("guildmage", "player-0", 2, 2);
    engine.state.players[0].battlefield = vec![guildmage.clone()];
    // Vizkopa Guildmage's created trigger, as compiled.
    let vizkopa = json!({
        "kind": "installCreatedTrigger",
        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
        "duration": { "kind": "untilEndOfCurrentTurn" },
        "oneShot": false,
        "event": { "kind": "lifeGained", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        "effects": [{
            "kind": "loseLife",
            "amount": { "kind": "decisionResult", "decisionId": "lifeGainedAmount" },
            "player": { "kind": "opponentsOf", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        }],
    });
    install(&mut engine, &vizkopa, &guildmage, "player-0");
    engine.enqueue_life_gained_triggers("player-1", 4);
    assert!(engine.state.stack.is_empty(), "an opponent's life gain");
    engine.enqueue_life_gained_triggers("player-0", 3);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].decisions["lifeGainedAmount"], 3);
    engine.state.stack.clear();

    // Dovin, Grand Arbiter's created trigger: one occurrence per combat
    // damage record of a creature its controller controls.
    let dovin = json!({
        "kind": "installCreatedTrigger",
        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
        "duration": { "kind": "untilEndOfCurrentTurn" },
        "oneShot": false,
        "event": {
            "kind": "damageDealtBySource",
            "combatOnly": true,
            "recipient": { "kind": "player", "player": { "kind": "eachPlayer" } },
            "source": {
                "kind": "permanents",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
        },
        "effects": [{
            "kind": "putCounters",
            "count": { "kind": "integer", "value": 1 },
            "counter": "loyalty",
            "permanent": { "kind": "self" },
        }],
    });
    install(&mut engine, &dovin, &guildmage, "player-0");
    engine.enqueue_source_damage_observers(
        "guildmage",
        Some("player-0"),
        DamageRecipientRef::Player("player-1"),
        2,
        false,
    );
    assert!(engine.state.stack.is_empty(), "noncombat damage");
    engine.enqueue_source_damage_observers(
        "guildmage",
        Some("player-0"),
        DamageRecipientRef::Player("player-1"),
        2,
        true,
    );
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPlayerId"],
        "player-1"
    );

    // A created event that reads a resolution decision is refused: the
    // decision is not captured by the created rule.
    let mut clone_saga = dovin.clone();
    clone_saga["event"]["source"]["where"] = json!({
        "kind": "chosenCardName",
        "decisionId": "resolutionCardName",
    });
    assert!(!effect_supported(&clone_saga));
}

#[test]
fn a_created_controlled_combat_damage_trigger_fires_once_per_damaged_player() {
    let rules = parsed_rules(
        "Jace, Cunning Castaway",
        "Legendary Planeswalker \u{2014} Jace",
        "+1: Whenever one or more creatures you control deal combat damage to a player this turn, draw a card, then discard a card.\n\u{2212}2: Create a 2/2 blue Illusion creature token with \"When this token becomes the target of a spell, sacrifice it.\"\n\u{2212}5: Create two tokens that are copies of Jace, except they're not legendary.",
    );
    let installer = rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().into_iter().flatten())
        .find(|effect| value_kind(effect) == Some("installCreatedTrigger"))
        .expect("the +1 installs a created trigger")
        .clone();
    let mut engine = test_engine(2);
    let jace = test_instance(
        "jace",
        test_definition("jace", "Legendary Planeswalker \u{2014} Jace"),
        "player-0",
    );
    let first = creature("first", "player-0", 2, 2);
    let second = creature("second", "player-0", 2, 2);
    engine.state.players[0].battlefield = vec![jace.clone(), first, second];
    install(&mut engine, &installer, &jace, "player-0");
    engine.enqueue_combat_damage_to_player_triggers("first", "player-1", 2);
    engine.enqueue_combat_damage_to_player_triggers("second", "player-1", 2);
    let draws = stack_effect_kinds(&engine)
        .into_iter()
        .filter(|(_, kind)| kind == "drawCards")
        .count();
    assert_eq!(draws, 1, "one or more creatures: one trigger");
}

// ---------------------------------------------------------------------------
// Damage dealt to the source itself (CR 120.1)
// ---------------------------------------------------------------------------

#[test]
fn a_source_deals_damage_to_itself_only_while_it_is_the_same_permanent() {
    let rules = parsed_rules(
        "Reckless Embermage",
        "Creature \u{2014} Human Wizard",
        "{1}{R}: This creature deals 1 damage to any target and 1 damage to itself.",
    );
    let rule = rule_with_effects(&rules, "activatedAbility").clone();
    let mut engine = test_engine(2);
    let embermage = creature("embermage", "player-0", 2, 2);
    engine.state.players[0].battlefield = vec![embermage.clone()];
    let mut source = stack_source(embermage.clone(), "player-0", "activatedAbility");
    source.ability_rule = Some(rule.clone());
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("the any-target slot")
        .to_string();
    source.targets.insert(
        target_id,
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
    );
    run_program(&mut engine, &rule["effects"], &source);
    assert_eq!(engine.state.players[1].life, 19);
    assert_eq!(permanent_of(&engine, "embermage").damage_marked, 1);

    // The source left the battlefield: it is dealt nothing, without error.
    let mut gone = test_engine(2);
    let self_damage = json!({
        "kind": "dealDamage",
        "amount": { "kind": "integer", "value": 3 },
        "recipient": { "kind": "self" },
        "source": { "kind": "self" },
    });
    run_effect(
        &mut gone,
        &self_damage,
        &stack_source(embermage, "player-0", "activatedAbility"),
    );
    assert!(gone.state.players[0].battlefield.is_empty());
}

#[test]
fn a_target_deals_damage_equal_to_its_power_back_to_the_source() {
    let rules = parsed_rules(
        "Tracker",
        "Creature \u{2014} Human",
        "{G}{G}, {T}: This creature deals damage equal to its power to target creature. That creature deals damage equal to its power to this creature.",
    );
    let rule = rule_with_effects(&rules, "activatedAbility").clone();
    let mut engine = test_engine(2);
    let tracker = creature("tracker", "player-0", 2, 5);
    engine.state.players[0].battlefield = vec![tracker.clone()];
    engine.state.players[1].battlefield = vec![creature("ogre", "player-1", 3, 5)];
    let mut source = stack_source(tracker, "player-0", "activatedAbility");
    source.ability_rule = Some(rule.clone());
    source.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "ogre".to_string(),
        },
    );
    run_program(&mut engine, &rule["effects"], &source);
    assert_eq!(permanent_of(&engine, "ogre").damage_marked, 2);
    assert_eq!(permanent_of(&engine, "tracker").damage_marked, 3);
    // Goblin Kaboomist's self damage composes with the coin flip program.
    parsed_rules(
        "Goblin Kaboomist",
        "Creature \u{2014} Goblin Warrior",
        "At the beginning of your upkeep, create a colorless artifact token named Land Mine with \"{R}, Sacrifice this token: This token deals 2 damage to target attacking creature without flying.\" Then flip a coin. If you lose the flip, this creature deals 2 damage to itself.",
    );
}

// ---------------------------------------------------------------------------
// Per-member counter counts
// ---------------------------------------------------------------------------

#[test]
fn oblivion_stone_destroys_unmarked_permanents_then_removes_every_fate_counter() {
    let rules = parsed_rules(
        "Oblivion Stone",
        "Artifact",
        "{4}, {T}: Put a fate counter on target permanent.\n{5}, {T}, Sacrifice this artifact: Destroy each nonland permanent without a fate counter on it, then remove all fate counters from all permanents.",
    );
    let wipe = rules
        .iter()
        .filter(|rule| value_kind(rule) == Some("activatedAbility"))
        .find(|rule| {
            rule["effects"]
                .as_array()
                .is_some_and(|effects| effects.len() == 2)
        })
        .expect("the wipe ability")
        .clone();
    let mut engine = test_engine(2);
    let mut kept = creature("kept", "player-0", 2, 2);
    kept.counters.insert("fate".to_string(), 2);
    let mut marked_land = test_instance(
        "marked-land",
        test_definition("marked-land", "Land"),
        "player-1",
    );
    marked_land.counters.insert("fate".to_string(), 1);
    let stone = test_instance("stone", test_definition("stone", "Artifact"), "player-0");
    // The stone stays on the battlefield here (its sacrifice is a cost);
    // as a nonland permanent without a fate counter it is destroyed too.
    engine.state.players[0].battlefield =
        vec![stone.clone(), kept, creature("doomed", "player-0", 2, 2)];
    engine.state.players[1].battlefield =
        vec![marked_land, creature("doomed-too", "player-1", 1, 1)];
    let mut source = stack_source(stone, "player-0", "activatedAbility");
    source.ability_rule = Some(wipe.clone());
    run_program(&mut engine, &wipe["effects"], &source);
    let ids = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .map(|permanent| permanent.instance_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["kept", "marked-land"]);
    for id in ["kept", "marked-land"] {
        assert_eq!(
            permanent_of(&engine, id)
                .counters
                .get("fate")
                .copied()
                .unwrap_or(0),
            0,
            "{id} lost all of its own fate counters"
        );
    }
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "counterRemoved")
            .count(),
        2
    );
}

#[test]
fn doubling_counters_reads_each_targets_own_count() {
    let rules = parsed_rules(
        "Solidarity of Heroes",
        "Instant",
        "Strive \u{2014} This spell costs {1}{G} more to cast for each target beyond the first.\nChoose any number of target creatures. Double the number of +1/+1 counters on each of them.",
    );
    let spell = rule_with_effects(&rules, "spellAbility").clone();
    let slot = spell["declaration"]["decisions"]
        .as_array()
        .expect("decisions")
        .iter()
        .find(|decision| value_kind(decision) == Some("chooseTargets"))
        .and_then(|decision| decision["id"].as_str())
        .expect("the target slot")
        .to_string();
    let mut engine = test_engine(2);
    let mut one = creature("one", "player-0", 2, 2);
    one.counters.insert("+1/+1".to_string(), 1);
    let mut three = creature("three", "player-0", 2, 2);
    three.counters.insert("+1/+1".to_string(), 3);
    let mut untargeted = creature("untargeted", "player-0", 2, 2);
    untargeted.counters.insert("+1/+1".to_string(), 2);
    engine.state.players[0].battlefield = vec![one, three, untargeted];
    let mut definition = test_definition("solidarity", "Instant");
    definition.set_printed_rules(rules.clone());
    let mut source = stack_source(
        test_instance("solidarity", definition, "player-0"),
        "player-0",
        "spellAbility",
    );
    source.ability_rule = Some(spell.clone());
    for (index, id) in ["one", "three"].iter().enumerate() {
        source.targets.insert(
            format!("{slot}:{index}"),
            TargetRef::Permanent {
                instance_id: (*id).to_string(),
            },
        );
    }
    run_program(&mut engine, &spell["effects"], &source);
    let count = |id: &str| permanent_of(&engine, id).counters.get("+1/+1").copied();
    assert_eq!(count("one"), Some(2));
    assert_eq!(count("three"), Some(6));
    assert_eq!(count("untargeted"), Some(2));
    // A count read on another object is not a member count.
    assert!(!effect_supported(&json!({
        "kind": "removeCounters",
        "counter": "fate",
        "count": { "kind": "counterCountOf", "counter": "fate", "object": { "kind": "chosenTargets", "id": "other" } },
        "permanent": { "kind": "eachPermanent" },
    })));
}

// ---------------------------------------------------------------------------
// Source-bound comparison operands
// ---------------------------------------------------------------------------

#[test]
fn champion_of_lambholt_compares_blockers_with_its_current_power() {
    let rules = parsed_rules(
        "Champion of Lambholt",
        "Creature \u{2014} Human Warrior",
        "Creatures with power less than this creature's power can't block creatures you control.\nWhenever another creature you control enters, put a +1/+1 counter on this creature.",
    );
    let mut engine = test_engine(2);
    let champion = with_rules(creature("champion", "player-0", 3, 3), rules);
    let attacker = creature("attacker", "player-0", 1, 1);
    let small = creature("small", "player-1", 2, 2);
    let equal = creature("equal", "player-1", 3, 3);
    engine.state.players[0].battlefield = vec![champion, attacker.clone()];
    engine.state.players[1].battlefield = vec![small.clone(), equal.clone()];
    assert!(!engine.can_block(&small, &attacker));
    assert!(engine.can_block(&equal, &attacker));
    // The operand is read now: a larger champion restricts more blockers.
    engine.state.players[0].battlefield[0]
        .counters
        .insert("+1/+1".to_string(), 1);
    assert!(!engine.can_block(&equal, &attacker));
    // An opponent's creature attacking is not a creature you control.
    let theirs = creature("their-attacker", "player-1", 1, 1);
    let our_blocker = creature("our-blocker", "player-0", 1, 1);
    assert!(engine.can_block(&our_blocker, &theirs));
}

#[test]
fn a_searched_card_compares_with_the_sources_power() {
    let rules = parsed_rules(
        "Sisay, Weatherlight Captain",
        "Legendary Creature \u{2014} Human Soldier",
        "Sisay gets +1/+1 for each color among other legendary permanents you control.\n{W}{U}{B}{R}{G}: Search your library for a legendary permanent card with mana value less than Sisay's power, put that card onto the battlefield, then shuffle.",
    );
    let search = rule_with_effects(&rules, "activatedAbility")["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .find(|effect| value_kind(effect) == Some("chooseCards"))
        .expect("the search choice")
        .clone();
    let engine = test_engine(2);
    let sisay = creature("sisay", "player-0", 3, 3);
    let source = stack_source(sisay, "player-0", "activatedAbility");
    let prepared = engine
        .prepare_runtime_object_filter(
            &search["candidates"]["where"],
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the source operand is read");
    assert!(prepared.to_string().contains("\"value\":3"), "{prepared}");
}

// ---------------------------------------------------------------------------
// ownedBy, extremumAmong and relation-filtered collections
// ---------------------------------------------------------------------------

#[test]
fn each_player_regains_control_of_what_they_own() {
    let rules = parsed_rules(
        "Homeward Path",
        "Land",
        "{T}: Add {C}.\n{T}: Each player gains control of all creatures they own.",
    );
    let rule = rules
        .iter()
        .find(|rule| value_kind(rule) == Some("activatedAbility"))
        .expect("the control ability")
        .clone();
    let mut engine = test_engine(2);
    let mut stolen = creature("stolen", "player-0", 2, 2);
    stolen.controller = "player-1".to_string();
    let mut borrowed = creature("borrowed", "player-1", 2, 2);
    borrowed.controller = "player-0".to_string();
    let path = test_instance("path", test_definition("path", "Land"), "player-0");
    engine.state.players[0].battlefield = vec![path.clone(), borrowed];
    engine.state.players[1].battlefield = vec![stolen];
    let mut source = stack_source(path, "player-0", "activatedAbility");
    source.ability_rule = Some(rule.clone());
    run_program(&mut engine, &rule["effects"], &source);
    assert_eq!(permanent_of(&engine, "stolen").controller, "player-0");
    assert_eq!(permanent_of(&engine, "borrowed").controller, "player-1");
    parsed_rules(
        "Brand",
        "Instant",
        "Gain control of all permanents you own. (This effect lasts indefinitely.)\nCycling {2} ({2}, Discard this card: Draw a card.)",
    );
}

#[test]
fn the_creatures_blocking_the_source_shrink_and_those_it_blocked_are_exiled() {
    let rules = parsed_rules(
        "Plague Wight",
        "Creature \u{2014} Zombie",
        "Whenever this creature becomes blocked, each creature blocking it gets -1/-1 until end of turn.",
    );
    let rule = rule_with_effects(&rules, "triggeredAbility").clone();
    let mut engine = test_engine(2);
    let wight = creature("wight", "player-0", 2, 1);
    engine.state.players[0].battlefield = vec![wight.clone()];
    engine.state.players[1].battlefield = vec![
        creature("wall", "player-1", 0, 4),
        creature("idle", "player-1", 2, 2),
    ];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "wight".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    engine.state.combat.blockers = vec![DeclaredBlocker {
        attacker_id: "wight".to_string(),
        blocker_id: "wall".to_string(),
    }];
    run_program(
        &mut engine,
        &rule["effects"],
        &stack_source(wight, "player-0", "triggeredAbility"),
    );
    assert_eq!(engine.current_toughness(permanent_of(&engine, "wall")), 3);
    assert_eq!(engine.current_toughness(permanent_of(&engine, "idle")), 2);

    let rules = parsed_rules(
        "Wall of Nets",
        "Creature \u{2014} Wall",
        "Defender (This creature can't attack.)\nAt end of combat, exile all creatures blocked by this creature.\nWhen this creature leaves the battlefield, return all cards exiled with it to the battlefield under their owners' control.",
    );
    let exile = rules
        .iter()
        .find(|rule| rule["effects"][0]["kind"] == "exilePermanent")
        .expect("the end-of-combat exile")
        .clone();
    let mut engine = test_engine(2);
    let wall = creature("nets", "player-0", 0, 7);
    engine.state.players[0].battlefield = vec![wall.clone()];
    engine.state.players[1].battlefield = vec![
        creature("caught", "player-1", 2, 2),
        creature("free", "player-1", 2, 2),
    ];
    engine.state.combat.attackers = ["caught", "free"]
        .into_iter()
        .map(|attacker| DeclaredAttacker {
            attacker_id: attacker.to_string(),
            defender: TargetRef::Player {
                player_id: "player-0".to_string(),
            },
            defending_player_id: "player-0".to_string(),
        })
        .collect();
    engine.state.combat.blockers = vec![DeclaredBlocker {
        attacker_id: "caught".to_string(),
        blocker_id: "nets".to_string(),
    }];
    run_program(
        &mut engine,
        &exile["effects"],
        &stack_source(wall, "player-0", "triggeredAbility"),
    );
    let remaining = engine.state.players[1]
        .battlefield
        .iter()
        .map(|permanent| permanent.instance_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(remaining, vec!["free"]);
}

#[test]
fn the_greatest_power_ranks_ties_together_and_reads_its_population() {
    let rules = parsed_rules(
        "Topple",
        "Sorcery",
        "Exile target creature with the greatest power among creatures on the battlefield. (If two or more creatures are tied for greatest power, target any one of them.)",
    );
    let spell = rule_with_effects(&rules, "spellAbility").clone();
    let mut candidates = spell["declaration"]["decisions"][0]["candidates"].clone();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("big", "player-0", 4, 4),
        creature("small", "player-0", 1, 1),
    ];
    engine.state.players[1].battlefield = vec![
        creature("tied", "player-1", 4, 2),
        test_instance("relic", test_definition("relic", "Artifact"), "player-1"),
    ];
    let topple = test_instance("topple", test_definition("topple", "Sorcery"), "player-0");
    engine
        .bind_source_candidates(&mut candidates, &topple)
        .expect("the ranking is prepared");
    let matches = |id: &str| {
        engine.permanent_matches_effective_filter(permanent_of(&engine, id), &candidates["where"])
    };
    assert!(matches("big"));
    assert!(matches("tied"));
    assert!(!matches("small"));
    assert!(!matches("relic"));
    // Among the creatures the ability's controller controls.
    let atom = json!({
        "kind": "extremumAmong",
        "property": "power",
        "extreme": "least",
        "population": "abilityController",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    let prepared = engine
        .prepare_extremum_among(&atom, "player-1")
        .expect("supported");
    assert_eq!(prepared["extremeIds"], json!(["tied"]));
    assert!(
        engine
            .prepare_extremum_among(
                &json!({ "kind": "extremumAmong", "property": "color" }),
                "player-0"
            )
            .is_none()
    );
    parsed_rules(
        "Primal Empathy",
        "Enchantment",
        "At the beginning of your upkeep, draw a card if you control a creature with the greatest power among creatures on the battlefield. Otherwise, put a +1/+1 counter on a creature you control.",
    );
}

// ---------------------------------------------------------------------------
// Forecast hand activation limits and the prepared designation
// ---------------------------------------------------------------------------

#[test]
fn a_forecast_ability_is_used_once_each_turn_from_the_hand() {
    let rules = parsed_rules(
        "Steeling Stance",
        "Instant",
        "Creatures you control get +1/+1 until end of turn.\nForecast \u{2014} {W}, Reveal this card from your hand: Target creature gets +1/+1 until end of turn. (Activate only during your upkeep and only once each turn.)",
    );
    let (rule_index, forecast) = rules
        .iter()
        .enumerate()
        .find(|(_, rule)| rule["activationZone"] == "hand")
        .map(|(index, rule)| (index, rule.clone()))
        .expect("the forecast ability");
    assert!(super::super::zone_of_function::hand_activation_limit_supported(&forecast));
    let mut engine = test_engine(2);
    let mut definition = test_definition("stance", "Instant");
    definition.set_printed_rules(rules.clone());
    let card = test_instance("stance", definition, "player-0");
    engine.state.players[0].hand.push(card.clone());
    assert!(!engine.activation_limit_reached(&card, &forecast, rule_index));
    engine.record_hand_activation_use(0, "hand", "stance", &forecast, rule_index);
    let in_hand = engine.state.players[0]
        .hand
        .iter()
        .find(|card| card.instance_id == "stance")
        .expect("still in hand")
        .clone();
    assert!(engine.activation_limit_reached(&in_hand, &forecast, rule_index));
    engine.state.turn_number += 1;
    assert!(!engine.activation_limit_reached(&in_hand, &forecast, rule_index));
    // A graveyard ability keeps no use record: its limit stays refused.
    let mut graveyard = forecast.clone();
    graveyard["activationZone"] = json!("graveyard");
    assert!(!super::super::zone_of_function::hand_activation_limit_supported(&graveyard));
}

#[test]
fn the_prepared_designation_is_a_permanent_state_atom() {
    let filter = json!({
        "kind": "not",
        "operand": { "kind": "hasDesignation", "designation": "prepared" },
    });
    assert!(permanent_filter_supported(&filter));
    let mut engine = test_engine(2);
    let mut prodigy = creature("prodigy", "player-0", 2, 2);
    engine.state.players[0].battlefield = vec![prodigy.clone()];
    assert!(engine.permanent_matches_effective_filter(&prodigy, &filter));
    prodigy.flags.insert("prepared".to_string(), true);
    assert!(!engine.permanent_matches_effective_filter(&prodigy, &filter));
    assert!(!permanent_filter_supported(&json!({
        "kind": "hasDesignation",
        "designation": "unprepared",
    })));
}
