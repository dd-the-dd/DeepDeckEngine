use super::*;

fn burst_lightning_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Burst Lightning".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some(
            "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("kicked-damage-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical");
    let rule = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(" deals "))
        .and_then(|ability| ability.rule.clone())
        .expect("Burst Lightning's damage rule parses");
    assert!(rule_is_executable(&rule));
    rule
}

fn resolve_burst_lightning(rule: &Value, kicked: bool) -> GameEngine {
    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: format!("stack:burst-lightning:{kicked}"),
        controller: "player-0".to_string(),
        card: test_instance(
            "burst-lightning",
            test_definition("burst-lightning", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::from([("kicked".to_string(), Value::Bool(kicked))]),
        targets: BTreeMap::from([(
            "targetDamageable".to_string(),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    };
    let mut decisions = stack_object.decisions.clone();
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut decisions,
            &mut EmeritusDecisionProvider,
        )
        .expect("the kicked damage branches resolve");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_same_rule_deals_the_base_or_kicked_amount_to_the_single_chosen_target() {
    let rule = burst_lightning_rule();

    let unkicked = resolve_burst_lightning(&rule, false);
    assert_eq!(unkicked.state.players[1].life, 18);

    let kicked = resolve_burst_lightning(&rule, true);
    assert_eq!(kicked.state.players[1].life, 16);
}

/// Parses one printed instant and returns its executable spell rule.
fn kicked_spell_rule(name: &str, mana_cost: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("kicked-antecedent-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{name}: {:?}", parsed.abilities);
    let rule = parsed
        .abilities
        .iter()
        .find_map(|ability| {
            ability
                .rule
                .as_ref()
                .filter(|rule| rule["kind"] == "spellAbility")
        })
        .cloned()
        .unwrap_or_else(|| panic!("{name} emits a spell ability"));
    assert!(rule_is_executable(&rule), "{name}: {rule}");
    rule
}

/// A card of `type_line`; a creature is 2/2, and `lifelink` gives it the
/// keyword (CR 702.15).
fn kicked_test_card(
    id: &str,
    type_line: &str,
    mana_cost: &str,
    owner: &str,
    lifelink: bool,
) -> CardInstance {
    let creature = type_line.starts_with("Creature");
    let rules = if lifelink {
        json!([{
            "kind": "keywordAbility",
            "ability": { "kind": "lifelink" },
            "source": { "kind": "self" }
        }])
    } else {
        json!([])
    };
    let definition: CardDefinition = serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line, "manaCost": mana_cost,
        "power": creature.then_some("2"),
        "toughness": creature.then_some("2"),
        "rules": rules,
    }))
    .expect("test definition is valid");
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

/// Puts `rule` on the stack as player 0's instant with its targets
/// designated at casting (CR 601.2c), then resolves it.
fn resolve_kicked_spell(
    engine: &mut GameEngine,
    rule: &Value,
    kicked: bool,
    targets: &[(&str, TargetRef)],
) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut stack = StackObject {
        id: format!("stack:kicked:{kicked}"),
        controller: "player-0".to_string(),
        card: kicked_test_card("kicked-spell", "Instant", "{U}", "player-0", false),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::from([("kicked".to_string(), Value::Bool(kicked))]),
        targets: targets
            .iter()
            .map(|(id, target)| (id.to_string(), target.clone()))
            .collect(),
    };
    for (id, target) in stack.targets.clone() {
        engine
            .capture_target_incarnation(&mut stack.decisions, &id, &target)
            .expect("the fixture target is designated");
    }
    engine.state.stack.push(stack);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the kicked spell resolves");
}

fn kicked_permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

/// Damage the 2/2 creature `id` received: marked on it, or lethal once it
/// left the battlefield.
fn damage_received(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| &player.battlefield)
        .find(|card| card.instance_id == id)
        .map_or(2, |card| card.damage_marked)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_kicked_pronoun_damage_source_is_the_spell_not_the_earlier_target() {
    // Jilt (CR 120.3, 608.2c): `it` after `If this spell was kicked` names
    // the spell. The returned creature has lifelink: had it dealt the
    // damage, its controller would gain life (CR 702.15b).
    let rule = kicked_spell_rule(
        "Jilt",
        "{1}{U}",
        "Kicker {1}{R} (You may pay an additional {1}{R} as you cast this spell.)\nReturn target creature to its owner's hand. If this spell was kicked, it deals 2 damage to another target creature.",
    );
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .expect("Jilt declares its targets");
    // `another target creature` (CR 115.3): never the returned creature.
    assert_eq!(decisions[1]["distinctFrom"], json!([decisions[0]["id"]]));
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(kicked_test_card(
        "returned",
        "Creature - Bat",
        "{1}{B}",
        "player-0",
        true,
    ));
    engine.state.players[1].battlefield.push(kicked_test_card(
        "damaged",
        "Creature - Bear",
        "{1}{G}",
        "player-1",
        false,
    ));
    resolve_kicked_spell(
        &mut engine,
        &rule,
        true,
        &[
            ("targetPermanent1", kicked_permanent("returned")),
            ("targetPermanent2", kicked_permanent("damaged")),
        ],
    );
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "returned")
    );
    assert_eq!(damage_received(&engine, "damaged"), 2);
    assert_eq!(engine.state.players[0].life, 20);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_kicked_spell_deals_damage_read_from_a_destroyed_target() {
    // Orim's Thunder: the spell deals the damage (CR 120.3) although the
    // destroyed permanent is gone; its mana value is last known
    // information (CR 608.2h). An unkicked cast deals none.
    let rule = kicked_spell_rule(
        "Orim's Thunder",
        "{2}{W}",
        "Kicker {R} (You may pay an additional {R} as you cast this spell.)\nDestroy target artifact or enchantment. If this spell was kicked, it deals damage equal to that permanent's mana value to target creature.",
    );
    for kicked in [true, false] {
        let mut engine = test_engine(2);
        engine.state.players[1].battlefield.push(kicked_test_card(
            "relic", "Artifact", "{2}", "player-1", false,
        ));
        engine.state.players[1].battlefield.push(kicked_test_card(
            "damaged",
            "Creature - Bear",
            "{1}{G}",
            "player-1",
            false,
        ));
        let mut targets = vec![("targetPermanent1", kicked_permanent("relic"))];
        if kicked {
            targets.push(("targetPermanent2", kicked_permanent("damaged")));
        }
        resolve_kicked_spell(&mut engine, &rule, kicked, &targets);
        assert!(
            !engine.state.players[1]
                .battlefield
                .iter()
                .any(|card| card.instance_id == "relic")
        );
        assert_eq!(
            damage_received(&engine, "damaged"),
            if kicked { 2 } else { 0 }
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_kicked_nominal_card_anaphor_keeps_the_declared_card_target() {
    // Zuko's Conviction: `that card` names the graveyard target in both
    // branches (CR 608.2c), so every cast declares it.
    let rule = kicked_spell_rule(
        "Zuko's Conviction",
        "{B}",
        "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nReturn target creature card from your graveyard to your hand. If this spell was kicked, instead put that card onto the battlefield tapped.",
    );
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .expect("the spell declares its target");
    assert_eq!(decisions.len(), 1);
    assert!(decisions[0].get("condition").is_none(), "{}", decisions[0]);
    for kicked in [true, false] {
        let mut engine = test_engine(2);
        engine.state.players[0].graveyard.push(kicked_test_card(
            "recruit",
            "Creature - Bear",
            "{1}{G}",
            "player-0",
            false,
        ));
        let target = TargetRef::Card {
            instance_id: "recruit".to_string(),
        };
        resolve_kicked_spell(&mut engine, &rule, kicked, &[("targetCard", target)]);
        let player = &engine.state.players[0];
        let entered = player
            .battlefield
            .iter()
            .find(|card| card.instance_id == "recruit");
        assert_eq!(entered.is_some(), kicked);
        assert!(entered.is_none_or(|card| card.tapped));
        assert_eq!(
            player.hand.iter().any(|card| card.instance_id == "recruit"),
            !kicked
        );
        assert!(
            !player
                .battlefield
                .iter()
                .any(|card| card.instance_id == "kicked-spell")
        );
    }
}
