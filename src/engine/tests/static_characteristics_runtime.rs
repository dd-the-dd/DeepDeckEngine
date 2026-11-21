//! Oracle text -> canonical IR -> runtime for the shared characteristic
//! predicates (color quantity CR 105.2, historic CR 700.6), continuous type
//! changes (CR 205.1, 305.6-305.7) and activation prohibitions (CR 602.5).
//! Each rule comes from the parser; the engine then reads effective
//! characteristics, so two of the new concepts are also composed together.
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

fn permanent(engine: &mut GameEngine, id: &str, type_line: &str, mana_cost: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    let instance = test_instance(id, definition, "player-0");
    engine.state.players[0].battlefield.push(instance.clone());
    instance
}

fn source(engine: &mut GameEngine, name: &str, type_line: &str, text: &str) -> CardInstance {
    let mut definition = test_definition(name, type_line);
    definition.rules = parsed_rules(name, type_line, text);
    let instance = test_instance(name, definition, "player-0");
    engine.state.players[0].battlefield.push(instance.clone());
    instance
}

fn mana_symbols(engine: &GameEngine, land: &CardInstance) -> BTreeSet<String> {
    let land = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|candidate| candidate.instance_id == land.instance_id)
        .expect("land on battlefield");
    engine
        .static_basic_land_mana(land)
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn color_quantity_predicates_read_effective_colors() {
    let rules = parsed_rules(
        "Tome of the Guildpact",
        "Artifact",
        "Whenever you cast a multicolored spell, draw a card.",
    );
    let multicolored = rules[0]["event"]["where"].clone();
    let mut engine = test_engine(2);
    let gold = permanent(&mut engine, "gold", "Creature - Elf", "{G}{W}");
    let mono = permanent(&mut engine, "mono", "Creature - Elf", "{G}");
    let golem = permanent(&mut engine, "golem", "Artifact Creature - Golem", "{3}");
    assert!(matches_card_filter(&gold.definition, &multicolored));
    assert!(!matches_card_filter(&mono.definition, &multicolored));

    // Unused sibling: `colorless` in a counted collection of an activated ability.
    let rules = parsed_rules(
        "Tomb of the Spirit Dragon",
        "Land",
        "{2}, {T}: You gain 1 life for each colorless creature you control.",
    );
    let colorless = rules[0]["effects"][0]["amount"]["where"].clone();
    assert!(engine.permanent_matches_effective_filter(&golem, &colorless));
    assert!(!engine.permanent_matches_effective_filter(&mono, &colorless));

    // The count follows the effective color set, not the printed mana cost.
    // Runtime fixture: the existing `setAllColors` primitive (its Oracle
    // copula `is all colors` is not part of this change).
    let mut painter = test_definition("painter", "Enchantment");
    painter.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": { "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "battlefield" } },
        "modifiers": [{
            "kind": "setAllColors",
            "objects": { "kind": "permanents", "where": { "kind": "cardTypeContains", "value": "Creature" } },
        }],
    })];
    engine.state.players[0]
        .battlefield
        .push(test_instance("painter", painter, "player-0"));
    let colorless_now = engine.permanent_matches_effective_filter(&golem, &colorless);
    let multicolored_now = engine.permanent_matches_effective_filter(&golem, &multicolored);
    assert!(!colorless_now && multicolored_now, "an all-colors golem");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn replacing_and_additive_land_types_change_subtypes_and_intrinsic_mana() {
    let mut engine = test_engine(2);
    let dual = permanent(&mut engine, "dual", "Land - Forest Plains", "");
    let utility = permanent(&mut engine, "utility", "Land", "");
    let basic = permanent(&mut engine, "basic", "Basic Land - Island", "");
    source(
        &mut engine,
        "Magus of the Moon",
        "Creature - Human Wizard",
        "Nonbasic lands are Mountains.",
    );
    let mountain = json!({ "kind": "subtypeContains", "value": "Mountain" });
    let forest = json!({ "kind": "subtypeContains", "value": "Forest" });
    assert!(engine.permanent_matches_effective_filter(&dual, &mountain));
    assert!(!engine.permanent_matches_effective_filter(&dual, &forest));
    assert_eq!(
        mana_symbols(&engine, &dual),
        BTreeSet::from(["R".to_string()])
    );
    assert_eq!(
        mana_symbols(&engine, &utility),
        BTreeSet::from(["R".to_string()])
    );
    assert_eq!(
        mana_symbols(&engine, &basic),
        BTreeSet::from(["U".to_string()])
    );

    // Additive land subtype (any basic type, not only Forest) adds its mana.
    let mut engine = test_engine(2);
    let utility = permanent(&mut engine, "utility", "Land", "");
    source(
        &mut engine,
        "Urborg, Tomb of Yawgmoth",
        "Legendary Land",
        "Each land is a Swamp in addition to its other land types.",
    );
    assert_eq!(
        mana_symbols(&engine, &utility),
        BTreeSet::from(["B".to_string()])
    );

    // Every basic land type, restricted to the controller's lands.
    let mut engine = test_engine(2);
    let utility = permanent(&mut engine, "utility", "Land", "");
    source(
        &mut engine,
        "Dryad of the Ilysian Grove",
        "Enchantment Creature - Nymph Dryad",
        "Lands you control are every basic land type in addition to their other types.",
    );
    let island = json!({ "kind": "subtypeContains", "value": "Island" });
    assert!(engine.permanent_matches_effective_filter(&utility, &island));
    assert_eq!(mana_symbols(&engine, &utility).len(), 5);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn activation_prohibition_reads_the_effective_source_types() {
    let mut engine = test_engine(2);
    let mut relic = test_definition("relic", "Artifact");
    relic.rules = vec![json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "amount": { "kind": "integer", "value": 1 } }],
    })];
    let relic = test_instance("relic", relic, "player-0");
    engine.state.players[0].battlefield.push(relic.clone());
    assert!(!engine.activation_prohibited(&relic, false));
    source(
        &mut engine,
        "Damping Matrix",
        "Artifact",
        "Activated abilities of artifacts and creatures can't be activated unless they're mana abilities.",
    );
    assert!(engine.activation_prohibited(&relic, false));
    assert!(
        !engine.activation_prohibited(&relic, true),
        "mana abilities are exempt"
    );

    // Unused combination: a land prohibition reaches a nonland artifact only
    // once a continuous type change makes it a land.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(relic.clone());
    source(
        &mut engine,
        "Probe Prohibition",
        "Enchantment",
        "Activated abilities of lands can't be activated.",
    );
    assert!(!engine.activation_prohibited(&relic, false));
    source(
        &mut engine,
        "Toph, the First Metalbender",
        "Legendary Creature - Human Warrior Ally",
        "Nontoken artifacts you control are lands in addition to their other types. (They don't gain the ability to {T} for mana.)",
    );
    assert!(engine.activation_prohibited(&relic, false));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn added_legendary_supertype_makes_a_permanent_historic() {
    let rules = parsed_rules(
        "Diligent Excavator",
        "Creature - Human Artificer",
        "Whenever you cast a historic spell, target player mills two cards. (Artifacts, legendaries, and Sagas are historic.)",
    );
    let historic = rules[0]["event"]["where"].clone();
    let mut engine = test_engine(2);
    let bear = permanent(&mut engine, "bear", "Creature - Bear", "{1}{G}");
    let saga = permanent(&mut engine, "saga", "Enchantment - Saga", "{1}{W}");
    assert!(!engine.permanent_matches_effective_filter(&bear, &historic));
    assert!(engine.permanent_matches_effective_filter(&saga, &historic));

    let mut aura = test_definition("wings", "Enchantment - Aura");
    aura.rules = parsed_rules(
        "On Serra's Wings",
        "Legendary Enchantment - Aura",
        "Enchanted creature is legendary, gets +1/+1, and has flying, vigilance, and lifelink.",
    );
    let mut aura = test_instance("wings", aura, "player-0");
    aura.attached_to = Some(bear.instance_id.clone());
    engine.state.players[0].battlefield.push(aura);
    assert!(engine.permanent_matches_effective_filter(&bear, &historic));
    assert!(engine.permanent_has_effective_keyword(&bear, "lifelink"));
}
