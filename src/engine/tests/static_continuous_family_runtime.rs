//! Oracle text -> canonical IR -> runtime for the static continuous family
//! of batch 9: a static copula that replaces card types and creature types
//! (CR 205.1a/b, layer 4) with ability loss (layer 6) and base power and
//! toughness (layer 7b) under counters (layer 7c); a land-subtype set that
//! replaces the rules text (CR 305.7); a quoted trigger granted to every
//! present object (CR 613.1f); a targeting prohibition (CR 115.4); a
//! resolving keyword choice (CR 608.2d); and an ordinal spell grant.
use super::*;

fn parsed_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    parsed
        .abilities
        .iter()
        .map(|ability| {
            let rule = ability.rule.clone().expect("canonical rule");
            assert!(rule_is_executable(&rule), "{text}: {rule:#}");
            rule
        })
        .collect()
}

fn place(engine: &mut GameEngine, id: &str, type_line: &str, owner: &str) -> CardInstance {
    let instance = test_instance(id, test_definition(id, type_line), owner);
    let index = engine
        .state
        .players
        .iter()
        .position(|player| player.id == owner)
        .expect("owner seat");
    engine.state.players[index]
        .battlefield
        .push(instance.clone());
    instance
}

fn source(engine: &mut GameEngine, name: &str, type_line: &str, text: &str) -> CardInstance {
    let mut definition = test_definition(name, type_line);
    definition.rules = parsed_rules(name, type_line, text);
    let instance = test_instance(name, definition, "player-0");
    engine.state.players[0].battlefield.push(instance.clone());
    instance
}

fn live(engine: &GameEngine, id: &str) -> CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|permanent| permanent.instance_id == id)
        .cloned()
        .expect("permanent on the battlefield")
}

fn live_mut<'a>(engine: &'a mut GameEngine, id: &str) -> &'a mut CardInstance {
    engine
        .state
        .players
        .iter_mut()
        .flat_map(|player| player.battlefield.iter_mut())
        .find(|permanent| permanent.instance_id == id)
        .expect("permanent on the battlefield")
}

fn has_type(engine: &GameEngine, id: &str, card_type: &str) -> bool {
    engine.permanent_has_card_type(&live(engine, id), card_type)
}

fn has_subtype(engine: &GameEngine, id: &str, subtype: &str) -> bool {
    engine.permanent_matches_effective_filter(
        &live(engine, id),
        &json!({ "kind": "subtypeContains", "value": subtype }),
    )
}

fn attach(engine: &mut GameEngine, aura: &str, host: &str) {
    live_mut(engine, aura).attached_to = Some(host.to_string());
}

/// Frogify: layer 4 sets the card types (an artifact creature stops being
/// an artifact) and replaces the creature types, layer 6 removes the
/// printed abilities, layer 7b sets the base power and toughness, and a
/// +1/+1 counter still applies on top in layer 7c.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn frogify_sets_types_then_abilities_then_base_stats_under_counters() {
    let mut engine = test_engine(2);
    let mut golem = test_definition("golem", "Artifact Creature - Golem");
    golem.power = Some("4".to_string());
    golem.toughness = Some("4".to_string());
    golem.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "flying" },
    })];
    let golem = test_instance("golem", golem, "player-1");
    engine.state.players[1].battlefield.push(golem);
    live_mut(&mut engine, "golem")
        .counters
        .insert("+1/+1".to_string(), 1);
    assert!(engine.permanent_has_effective_keyword(&live(&engine, "golem"), "flying"));

    source(
        &mut engine,
        "Frogify",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature loses all abilities and is a blue Frog creature with base power and toughness 1/1. (It loses all other card types and creature types.)",
    );
    attach(&mut engine, "Frogify", "golem");

    assert!(has_type(&engine, "golem", "Creature"));
    assert!(!has_type(&engine, "golem", "Artifact"));
    assert!(has_subtype(&engine, "golem", "Frog"));
    assert!(!has_subtype(&engine, "golem", "Golem"));
    assert!(!engine.permanent_has_effective_keyword(&live(&engine, "golem"), "flying"));
    let golem = live(&engine, "golem");
    assert_eq!(
        (
            engine.current_power(&golem),
            engine.current_toughness(&golem)
        ),
        (2, 2)
    );

    // Without the Aura's host the same creature keeps its own types.
    live_mut(&mut engine, "Frogify").attached_to = None;
    assert!(has_type(&engine, "golem", "Artifact"));
    assert!(has_subtype(&engine, "golem", "Golem"));
    assert!(!has_subtype(&engine, "golem", "Frog"));
}

/// Lignify (unused sibling of the same lowering): a creature type named
/// without card types replaces only the creature types; the card types stay.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn lignify_replaces_creature_types_and_keeps_card_types() {
    let mut engine = test_engine(2);
    place(
        &mut engine,
        "golem",
        "Artifact Creature - Golem",
        "player-1",
    );
    source(
        &mut engine,
        "Lignify",
        "Kindred Enchantment - Treefolk Aura",
        "Enchant creature\nEnchanted creature is a Treefolk with base power and toughness 0/4 and loses all abilities.",
    );
    attach(&mut engine, "Lignify", "golem");
    assert!(has_type(&engine, "golem", "Artifact"));
    assert!(has_type(&engine, "golem", "Creature"));
    assert!(has_subtype(&engine, "golem", "Treefolk"));
    assert!(!has_subtype(&engine, "golem", "Golem"));
}

/// Song of the Dryads: the permanent becomes a land whose only land type is
/// Forest (CR 305.7); its card types are set and its rules text is gone.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn song_of_the_dryads_makes_a_colorless_forest_land() {
    let mut engine = test_engine(2);
    let mut bear = test_definition("bear", "Creature - Bear");
    bear.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "trample" },
    })];
    engine.state.players[1]
        .battlefield
        .push(test_instance("bear", bear, "player-1"));
    source(
        &mut engine,
        "Song of the Dryads",
        "Enchantment - Aura",
        "Enchant permanent\nEnchanted permanent is a colorless Forest land.",
    );
    attach(&mut engine, "Song of the Dryads", "bear");
    assert!(has_type(&engine, "bear", "Land"));
    assert!(!has_type(&engine, "bear", "Creature"));
    assert!(has_subtype(&engine, "bear", "Forest"));
    assert!(!has_subtype(&engine, "bear", "Bear"));
    let bear = live(&engine, "bear");
    assert_eq!(engine.effective_rules(&bear).printed().count(), 0);
}

/// A quoted trigger granted to `creatures you control` reaches a creature
/// that entered after the grantor: the grant is a continuous effect on the
/// current population (CR 611.3a, 613.1f), not a one-time copy.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn granted_trigger_reaches_a_creature_entering_after_the_grantor() {
    let mut engine = test_engine(2);
    source(
        &mut engine,
        "Battle Cry Probe",
        "Enchantment",
        "Creatures you control have \"Whenever this creature attacks, it gets +1/+0 until end of turn.\"",
    );
    let late = place(&mut engine, "late", "Creature - Soldier", "player-0");
    let opposing = place(&mut engine, "opposing", "Creature - Soldier", "player-1");
    let granted = |engine: &GameEngine, permanent: &CardInstance| {
        engine
            .effective_rules(&live(engine, &permanent.instance_id))
            .iter()
            .filter(|rule| value_kind(rule) == Some("triggeredAbility"))
            .count()
    };
    assert_eq!(granted(&engine, &late), 1);
    assert_eq!(granted(&engine, &opposing), 0);
}

/// Canopy Cover: the enchanted creature can't be targeted by an opponent's
/// spell or ability; its controller's own spells still may.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn canopy_cover_forbids_only_opponents_targeting() {
    let mut engine = test_engine(2);
    place(&mut engine, "bear", "Creature - Bear", "player-0");
    source(
        &mut engine,
        "Canopy Cover",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature can't be blocked except by creatures with flying or reach.\nEnchanted creature can't be the target of spells or abilities your opponents control.",
    );
    attach(&mut engine, "Canopy Cover", "bear");
    let bolt = test_definition("bolt", "Instant");
    let bear = live(&engine, "bear");
    assert!(engine.static_targeting_prohibited(&bear, "player-1", &bolt, true));
    assert!(engine.static_targeting_prohibited(&bear, "player-1", &bolt, false));
    assert!(!engine.static_targeting_prohibited(&bear, "player-0", &bolt, true));
}

/// `gains your choice of ...`: the controller chooses one keyword while the
/// ability resolves, and only that keyword is granted until end of turn.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn keyword_choice_grants_only_the_chosen_keyword() {
    let mut engine = test_engine(2);
    let rules = parsed_rules(
        "Choice Probe",
        "Creature - Construct",
        "{1}: This creature gains your choice of flying, vigilance, or lifelink until end of turn.",
    );
    let mut definition = test_definition("probe", "Creature - Construct");
    definition.rules = rules.clone();
    let probe = test_instance("probe", definition, "player-0");
    engine.state.players[0].battlefield.push(probe.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let stack = StackObject {
        id: "stack:choice".to_string(),
        controller: "player-0".to_string(),
        card: live(&engine, "probe"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rules[0].clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rules[0]["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NamedOptionDecisionProvider {
                option: "vigilance",
            },
        )
        .expect("the choice and the grant resolve");
    let probe = live(&engine, "probe");
    assert!(engine.permanent_has_effective_keyword(&probe, "vigilance"));
    assert!(!engine.permanent_has_effective_keyword(&probe, "flying"));
    assert!(!engine.permanent_has_effective_keyword(&probe, "lifelink"));
}

/// `The first creature spell you cast each turn has convoke`: the grant
/// applies until a matching spell was cast this turn.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn first_spell_grant_closes_after_a_matching_cast() {
    let mut engine = test_engine(2);
    source(
        &mut engine,
        "Convoke Probe",
        "Enchantment",
        "The first creature spell you cast each turn has convoke.",
    );
    let creature = test_definition("elf", "Creature - Elf");
    let sorcery = test_definition("ritual", "Sorcery");
    assert!(engine.card_has_convoke(&creature, "player-0"));
    assert!(!engine.card_has_convoke(&sorcery, "player-0"));
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("cast-elf".to_string()),
        json!({ "typeLine": "Creature - Elf", "cardName": "Elf" }),
    );
    assert!(!engine.card_has_convoke(&creature, "player-0"));
}
