use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"rules":rules}))
        .expect("bounded synthetic definition")
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Land", vec![]);
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        601,
    )
    .expect("bounded engine")
}
fn mana_rule(value: Value) -> Value {
    json!({"kind":"manaAbility","source":{"kind":"self"},"costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":value}]})
}
fn land(id: &str, owner: &str, value: Value) -> CardInstance {
    let rule = mana_rule(value);
    assert!(rule_is_executable(&rule));
    let mut p = card_instance(owner, &definition(id, "Land", vec![rule]), 0);
    p.instance_id = id.into();
    p
}
fn pool(id: &str, owner: &str) -> CardInstance {
    land(
        id,
        owner,
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
    )
}
fn expect_error(engine: &GameEngine) -> ManaProjectionContextError {
    match ManaProjectionContext::new(engine) {
        Ok(_) => panic!("expected explicit unsupported projection"),
        Err(e) => e,
    }
}
fn projected(kind: ManaType) -> ManaTypeProjection {
    ManaTypeProjection::Types(BTreeSet::from([kind]))
}

#[test]
fn mana_context_ungrounded_cycles_are_defined_empty_without_legality_claims() {
    for count in [1, 2] {
        let mut e = engine();
        e.state.players[0].battlefield = (0..count)
            .map(|i| pool(&format!("pool{i}"), "player-0"))
            .collect();
        let c = ManaProjectionContext::new(&e).expect("undefined is semantic, not unsupported");
        assert_eq!(
            c.types_for_controller("player-0").unwrap(),
            ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
        );
        for i in 0..count {
            assert_eq!(
                *c.types_for_source(&format!("pool{i}")).unwrap(),
                ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
            );
        }
        // No availability=false assertion: empty production is not activation illegality.
    }
}

#[test]
fn mana_context_grounded_cycle_and_actual_land_controller_scope() {
    let mut e = engine();
    e.state.players[0].battlefield = vec![
        pool("pool0", "player-0"),
        pool("pool1", "player-0"),
        land("wastes", "player-0", json!("{C}")),
    ];
    let mut nonland = land("nonland", "player-0", json!("{U}"));
    nonland.definition.type_line = "Creature".into();
    nonland.definition.power = Some("1".into());
    nonland.definition.toughness = Some("1".into());
    e.state.players[0].battlefield.push(nonland);
    e.state.players[1].battlefield = vec![land("opposing", "player-1", json!("{R}"))];
    let c = ManaProjectionContext::new(&e).unwrap();
    for id in ["pool0", "pool1", "wastes"] {
        assert_eq!(
            *c.types_for_source(id).unwrap(),
            projected(ManaType::Colorless)
        );
    }
    assert_eq!(
        c.types_for_controller("player-0").unwrap(),
        projected(ManaType::Colorless)
    );
    assert_eq!(
        c.types_for_controller("player-1").unwrap(),
        projected(ManaType::Red)
    );
}

#[test]
fn mana_context_opposing_cycles_receive_only_the_grounded_color() {
    let mut e = engine();
    let orchard = |id, owner| {
        land(
            id,
            owner,
            json!({"kind":"manaColorsLandsOpponentsControlCouldProduce","amount":1}),
        )
    };
    e.state.players[0].battlefield = vec![
        orchard("a", "player-0"),
        land("green", "player-0", json!("{G}")),
    ];
    e.state.players[1].battlefield = vec![orchard("b", "player-1")];
    let c = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *c.types_for_source("a").unwrap(),
        projected(ManaType::Green)
    );
    assert_eq!(
        *c.types_for_source("b").unwrap(),
        projected(ManaType::Green)
    );
}

#[test]
fn mana_context_effective_subtype_replaces_obsolete_printed_output() {
    let mut e = engine();
    let modifier = json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"setLandSubtype","objects":{"kind":"permanents","where":{"kind":"nameEquals","value":"changed"}},"subtype":"Swamp"}]});
    assert!(rule_is_executable(&modifier));
    e.state.players[0].battlefield = vec![
        pool("pool", "player-0"),
        land("changed", "player-0", json!("{U}")),
        card_instance(
            "player-0",
            &definition("grant", "Enchantment", vec![modifier]),
            0,
        ),
    ];
    let c = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *c.types_for_source("changed").unwrap(),
        projected(ManaType::Black)
    );
    assert_eq!(
        *c.types_for_source("pool").unwrap(),
        projected(ManaType::Black)
    );
}

#[test]
fn mana_context_ignores_payment_and_distinguishes_zero_amount() {
    let mut e = engine();
    let mut p = land("costly", "player-0", json!("{G}"));
    p.tapped = true;
    p.definition.rules[0]["costs"] = json!([{"kind":"tap","object":{"kind":"self"}},
        {"kind":"payMana","manaCost":"{9}"},{"kind":"payLife","amount":{"kind":"integer","value":30}}]);
    e.state.players[0].battlefield = vec![
        p,
        land(
            "zero",
            "player-0",
            json!({"kind":"fixedMana","symbol":"R","amount":{"kind":"integer","value":0}}),
        ),
    ];
    let c = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *c.types_for_source("costly").unwrap(),
        projected(ManaType::Green)
    );
    assert_eq!(
        *c.types_for_source("zero").unwrap(),
        ManaTypeProjection::NoMana(NoManaReason::ZeroAmount)
    );
}

#[test]
fn mana_context_nested_choice_queries_use_the_same_fixed_point() {
    let mut e = engine();
    e.state.players[0].battlefield = vec![
        land(
            "nested",
            "player-0",
            json!({"kind":"chooseOne","options":[
        {"kind":"manaTypesLandsYouControlCouldProduce","amount":1}]}),
        ),
        land("anchor", "player-0", json!("{C}")),
    ];
    let c = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *c.types_for_source("nested").unwrap(),
        projected(ManaType::Colorless)
    );
}

#[test]
fn mana_context_cap_provenance_survives_as_engine_error() {
    let mut e = engine();
    let mut p = land("bad-prefix", "player-0", json!("{G}"));
    let effects = p.definition.rules[0]["effects"].as_array_mut().unwrap();
    effects.insert(0,json!({"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":1}}));
    assert!(rule_is_executable(&p.definition.rules[0]));
    e.state.players[0].battlefield = vec![p];
    let before = serde_json::to_value(&e.state).expect("state snapshot");
    let error = expect_error(&e);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(error.state, "mana.projection.program");
    assert_eq!(error.rule_index, Some(0));
    assert_eq!(error.source_instance_id, "bad-prefix");
    assert_eq!(error.detail, "StatefulPrefix");
    let error = error.into_engine_error();
    assert!(error.message.contains("StatefulPrefix"));
    assert!(error.message.contains("bad-prefix"));
}

#[test]
fn mana_context_missing_numeric_and_sacrifice_bindings_are_not_zero() {
    for (value, state) in [
        (
            json!({"kind":"fixedMana","symbol":"G","amount":{"kind":"decisionResult","decisionId":"unbound"}}),
            "mana.projection.numeric-binding",
        ),
        (
            json!({"kind":"manaTypesOfSacrificedPermanent","decisionId":"unbound","amount":1}),
            "mana.projection.sacrificed-binding",
        ),
    ] {
        let mut e = engine();
        e.state.players[0].battlefield = vec![land("unbound", "player-0", value)];
        let error = expect_error(&e);
        assert_eq!(error.state, state);
        assert_eq!(error.source_instance_id, "unbound");
    }
}

#[test]
fn mana_context_commanderless_or_unchosen_color_is_undefined_not_all_colors() {
    for value in [
        json!({"kind":"chooseCommanderColor","amount":1}),
        json!({"kind":"storedColor","decisionId":"absent","amount":1}),
    ] {
        let mut e = engine();
        e.state.players[0].battlefield = vec![
            land("undefined", "player-0", value),
            pool("pool", "player-0"),
        ];
        let c = ManaProjectionContext::new(&e).unwrap();
        assert_eq!(
            *c.types_for_source("undefined").unwrap(),
            ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
        );
        assert_eq!(
            *c.types_for_source("pool").unwrap(),
            ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
        );
    }
}

#[test]
fn mana_context_lost_abilities_do_not_restore_intrinsic_mana() {
    let mut e = engine();
    let mut forest = land("lost", "player-0", json!("{G}"));
    forest.definition.type_line = "Basic Land - Forest".into();
    e.state.players[0].battlefield = vec![forest, pool("pool", "player-0")];
    e.state
        .rule_modifiers
        .push(json!({"kind":"loseAllAbilities","cardInstanceId":"lost"}));
    let c = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *c.types_for_source("lost").unwrap(),
        ManaTypeProjection::NoMana(NoManaReason::NoManaAbility)
    );
    assert_eq!(
        *c.types_for_source("pool").unwrap(),
        ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
    );
}

#[test]
fn source_projection_rejects_opaque_rules_through_the_shared_executable_contract() {
    for rule in [
        json!({"kind":"unimplementedSourceProgram"}),
        json!({"kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
            "effects":[{"kind":"resolveTriggeredInstruction","instruction":"unimplemented program"}]}),
    ] {
        assert!(!rule_is_executable(&rule));
        let mut e = engine();
        let mut source = land("opaque-source", "player-0", json!("{G}"));
        source.definition.rules.push(rule);
        e.state.players[0].battlefield = vec![source];
        let before = e.state.clone();
        let failure = expect_error(&e);
        assert_eq!(failure.state, "mana.projection.opaque-producer");
        assert_eq!(failure.source_instance_id, "opaque-source");
        assert_eq!(e.state, before, "certification is read-only");
    }
}

#[test]
fn executable_metadata_and_nonproducer_programs_preserve_grounded_types() {
    let safe = [
        json!({"kind":"rulesMarker","text":"Projection metadata","colorIdentity":[]}),
        json!({"kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
            "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]}),
    ];
    assert!(safe.iter().all(rule_is_executable));
    let mut e = engine();
    let mut source = land("safe-source", "player-0", json!("{G}"));
    source.definition.rules.extend(safe);
    e.state.players[0].battlefield = vec![source];
    let context =
        ManaProjectionContext::new(&e).expect("valid metadata/nonproducer must not become opaque");
    assert_eq!(
        *context.types_for_source("safe-source").unwrap(),
        projected(ManaType::Green)
    );
}

#[test]
fn executable_nonprojectable_producer_still_requires_its_domain_cap() {
    let fixed_symbols = json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
        "effects":[{"kind":"addFixedManaSymbols","player":{"kind":"abilityController"},"symbols":["R"]}]});
    assert!(
        rule_is_executable(&fixed_symbols),
        "runtime executability alone does not certify a mana type projection"
    );
    for rule in [fixed_symbols, json!({"kind":"firebending","count":1})] {
        let mut e = engine();
        let mut source = land("bounded-producer", "player-0", json!("{G}"));
        source.definition.rules.push(rule);
        e.state.players[0].battlefield = vec![source];
        let failure = expect_error(&e);
        assert_eq!(failure.state, "mana.projection.opaque-producer");
    }
}
