//! Oracle text -> canonical IR -> runtime for the shared `becomes` frame:
//! card types (CR 205.1a/b, layer 4), creature subtypes, colors (CR 105.3,
//! layer 5), granted keywords (layer 6), base power/toughness (layer 7b) and
//! ability loss, each recorded on the affected object with its duration.
use super::*;

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    let rule = parsed
        .abilities
        .last()
        .and_then(|ability| ability.rule.clone())
        .expect("canonical rule");
    assert!(rule_is_executable(&rule), "{text}: {rule:#}");
    rule
}

fn place(engine: &mut GameEngine, id: &str, type_line: &str) -> CardInstance {
    let instance = test_instance(id, test_definition(id, type_line), "player-0");
    engine.state.players[0].battlefield.push(instance.clone());
    instance
}

fn resolve(engine: &mut GameEngine, rule: &Value, source: &CardInstance, target: Option<&str>) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = &live(engine, &source.instance_id);
    let stack = StackObject {
        id: "stack:animation".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(rule["kind"].as_str().unwrap_or("spellAbility").to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: target
            .map(|id| {
                BTreeMap::from([(
                    "targetPermanent".to_string(),
                    TargetRef::Permanent {
                        instance_id: id.to_string(),
                    },
                )])
            })
            .unwrap_or_default(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("animation primitives resolve");
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

fn has_type(engine: &GameEngine, id: &str, card_type: &str) -> bool {
    engine.permanent_has_card_type(&live(engine, id), card_type)
}

fn has_subtype(engine: &GameEngine, id: &str, subtype: &str) -> bool {
    engine.permanent_matches_effective_filter(
        &live(engine, id),
        &json!({ "kind": "subtypeContains", "value": subtype }),
    )
}

fn stats(engine: &GameEngine, id: &str) -> (i32, i32) {
    let permanent = live(engine, id);
    (
        engine.current_power(&permanent),
        engine.current_toughness(&permanent),
    )
}

fn colors(engine: &GameEngine, id: &str) -> Vec<String> {
    engine
        .permanent_color_symbols(&live(engine, id))
        .into_iter()
        .collect()
}

/// Real card (Restless Reef): every layer of the animation applies until the
/// cleanup step, and the land is restored afterwards (CR 514.2).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn restless_land_animation_applies_each_layer_until_cleanup() {
    let rule = parsed_rule(
        "Restless Reef",
        "Land",
        "{2}{U}{B}: Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.",
    );
    let mut engine = test_engine(2);
    let reef = place(&mut engine, "reef", "Land");
    resolve(&mut engine, &rule, &reef, None);

    assert!(has_type(&engine, "reef", "Creature"));
    assert!(has_type(&engine, "reef", "Land"));
    assert!(has_subtype(&engine, "reef", "Shark"));
    assert_eq!(stats(&engine, "reef"), (4, 4));
    assert_eq!(colors(&engine, "reef"), ["B", "U"]);
    assert!(engine.permanent_has_effective_keyword(&live(&engine, "reef"), "deathtouch"));

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup");
    assert!(!has_type(&engine, "reef", "Creature"));
    assert!(has_type(&engine, "reef", "Land"));
    assert!(!has_subtype(&engine, "reef", "Shark"));
    assert!(colors(&engine, "reef").is_empty());
    assert!(!engine.permanent_has_effective_keyword(&live(&engine, "reef"), "deathtouch"));
}

/// Without a retention clause the card types are set (CR 205.1a): an
/// animated enchantment stops being an enchantment until the effect ends.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn card_type_setting_replaces_prior_types_until_cleanup() {
    let rule = parsed_rule(
        "Still Life Variant",
        "Enchantment",
        "{G}{G}: This enchantment becomes a 4/3 Centaur creature until end of turn.",
    );
    let mut engine = test_engine(2);
    let still_life = place(&mut engine, "still-life", "Enchantment");
    resolve(&mut engine, &rule, &still_life, None);
    assert!(has_type(&engine, "still-life", "Creature"));
    assert!(!has_type(&engine, "still-life", "Enchantment"));
    assert!(has_subtype(&engine, "still-life", "Centaur"));
    assert_eq!(stats(&engine, "still-life"), (4, 3));

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup");
    assert!(has_type(&engine, "still-life", "Enchantment"));
    assert!(!has_type(&engine, "still-life", "Creature"));
}

/// Real card (Woodwraith Corrupter): an open-ended targeted animation lasts
/// past cleanup, keeps the land subtype (CR 205.1b) and counters still apply
/// on top of the layer-7b base (CR 613.4b-c).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn open_ended_targeted_animation_persists_and_counters_apply_on_top() {
    let rule = parsed_rule(
        "Woodwraith Corrupter",
        "Creature - Elemental Horror",
        "{1}{B}{G}, {T}: Target Forest becomes a 4/4 black and green Elemental Horror creature. It's still a land.",
    );
    let mut engine = test_engine(2);
    let corrupter = place(&mut engine, "corrupter", "Creature - Elemental Horror");
    place(&mut engine, "forest", "Basic Land - Forest");
    resolve(&mut engine, &rule, &corrupter, Some("forest"));
    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup");

    assert!(has_type(&engine, "forest", "Creature"));
    assert!(has_type(&engine, "forest", "Land"));
    assert!(has_subtype(&engine, "forest", "Horror"));
    assert_eq!(colors(&engine, "forest"), ["B", "G"]);
    let (player, index) = engine.permanent_position("forest").expect("forest");
    engine.state.players[player].battlefield[index]
        .counters
        .insert("+1/+1".to_string(), 1);
    assert_eq!(stats(&engine, "forest"), (5, 5));
}

/// Real card (Humble): ability loss removes keywords (layer 6) while the new
/// base power/toughness, a characteristic, still applies (layer 7b).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn ability_loss_keeps_the_new_base_power_toughness() {
    let rule = parsed_rule(
        "Humble",
        "Instant",
        "Until end of turn, target creature loses all abilities and has base power and toughness 0/1.",
    );
    let mut engine = test_engine(2);
    let spell = place(&mut engine, "humble", "Instant");
    let mut flier = test_definition("flier", "Creature - Bird");
    flier.power = Some("3".to_string());
    flier.toughness = Some("3".to_string());
    flier.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "flying" },
    })];
    engine.state.players[1]
        .battlefield
        .push(test_instance("flier", flier, "player-1"));
    assert!(engine.permanent_has_effective_keyword(&live(&engine, "flier"), "flying"));
    resolve(&mut engine, &rule, &spell, Some("flier"));
    assert!(!engine.permanent_has_effective_keyword(&live(&engine, "flier"), "flying"));
    assert_eq!(stats(&engine, "flier"), (0, 1));

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup");
    engine.state.turn_number += 1;
    assert!(engine.permanent_has_effective_keyword(&live(&engine, "flier"), "flying"));
    assert_eq!(stats(&engine, "flier"), (3, 3));
}

/// Previously unused combination: a battlefield collection subject is locked
/// in once, then every primitive applies to each land (CR 611.2c).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn collection_animation_applies_to_each_locked_in_land() {
    let rule = parsed_rule(
        "Collective Awakening",
        "Sorcery",
        "Until end of turn, each land you control becomes a 3/3 Elemental creature with haste. It's still a land.",
    );
    let mut engine = test_engine(2);
    let spell = place(&mut engine, "awakening", "Sorcery");
    place(&mut engine, "forest", "Basic Land - Forest");
    place(&mut engine, "island", "Basic Land - Island");
    resolve(&mut engine, &rule, &spell, None);
    for land in ["forest", "island"] {
        assert!(has_type(&engine, land, "Creature"));
        assert!(has_subtype(&engine, land, "Elemental"));
        assert_eq!(stats(&engine, land), (3, 3));
        assert!(engine.permanent_has_effective_keyword(&live(&engine, land), "haste"));
    }
}
