//! Runtime proofs for C4 quantities and result reuse: the Oracle text is
//! compiled, then its primitive IR is executed by the ordinary evaluators.
use super::*;

fn compile(name: &str, type_line: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("quantity-result-reuse-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    let rule = parsed
        .abilities
        .last()
        .and_then(|ability| ability.rule.clone())
        .expect("canonical rule");
    assert!(rule_is_executable(&rule), "{name}: {rule:#}");
    rule
}

fn card(id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), owner)
}

fn resolve(engine: &mut GameEngine, rule: &Value) {
    let stack_object = StackObject {
        id: "stack:quantity-result-reuse".to_string(),
        controller: "player-0".to_string(),
        card: card("quantity-spell", "Sorcery", "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut stack_object.decisions.clone(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the primitive sequence resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn destroyed_this_way_counts_exactly_the_destroyed_permanents() {
    // Fumigate (CR 608.2c): the life gain reads the destroy action's result.
    let rule = compile(
        "Fumigate",
        "Sorcery",
        "Destroy all creatures. You gain 1 life for each creature destroyed this way.",
    );
    let mut engine = test_engine(2);
    let mut indestructible = card("indestructible-creature", "Creature", "player-1");
    indestructible.definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "indestructible" },
    }));
    engine.state.players[0].battlefield = vec![
        card("own-creature", "Creature", "player-0"),
        card("own-artifact", "Artifact", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![
        card("their-creature", "Creature", "player-1"),
        indestructible,
    ];
    let life = engine.state.players[0].life;
    resolve(&mut engine, &rule);
    // Two creatures were destroyed; the indestructible one was not counted.
    assert_eq!(engine.state.players[0].life, life + 2);
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn milled_this_way_filters_only_the_milled_cards() {
    let rule = compile(
        "Sibling Mill",
        "Sorcery",
        "Mill four cards. You gain 1 life for each creature card milled this way.",
    );
    let mut engine = test_engine(2);
    // A creature already in the graveyard is not part of the result.
    engine.state.players[0].graveyard = vec![card("old-creature", "Creature", "player-0")];
    engine.state.players[0].library = vec![
        card("milled-creature-a", "Creature", "player-0"),
        card("milled-land", "Land", "player-0"),
        card("milled-creature-b", "Artifact Creature", "player-0"),
        card("milled-instant", "Instant", "player-0"),
        card("unmilled-creature", "Creature", "player-0"),
    ];
    let life = engine.state.players[0].life;
    resolve(&mut engine, &rule);
    assert_eq!(engine.state.players[0].graveyard.len(), 5);
    assert_eq!(engine.state.players[0].life, life + 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn zone_scoped_counts_read_the_holder_zone_at_resolution() {
    let rule = compile(
        "Sibling Graveyard Count",
        "Sorcery",
        "You gain 1 life for each creature card in your graveyard.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard = vec![
        card("own-creature", "Creature", "player-0"),
        card("own-land", "Land", "player-0"),
        card("own-artifact-creature", "Artifact Creature", "player-0"),
    ];
    // Another player's graveyard is not `your graveyard`.
    engine.state.players[1].graveyard = vec![card("their-creature", "Creature", "player-1")];
    let life = engine.state.players[0].life;
    resolve(&mut engine, &rule);
    assert_eq!(engine.state.players[0].life, life + 2);

    let rule = compile(
        "Sibling All Graveyards",
        "Sorcery",
        "You gain 1 life for each creature card in all graveyards.",
    );
    let life = engine.state.players[0].life;
    resolve(&mut engine, &rule);
    assert_eq!(engine.state.players[0].life, life + 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn static_zone_counts_and_where_x_deltas_are_continuously_evaluated() {
    let knight = compile(
        "Knight of the Reliquary",
        "Creature — Human Knight",
        "This creature gets +1/+1 for each land card in your graveyard.",
    );
    let shadow = compile(
        "Death's Shadow",
        "Creature — Avatar",
        "This creature gets -X/-X, where X is your life total.",
    );
    let mut engine = test_engine(2);
    let mut knight_definition = test_definition("knight", "Creature");
    knight_definition.rules.push(knight);
    let knight = test_instance("knight", knight_definition, "player-0");
    let mut shadow_definition = test_definition("shadow", "Creature");
    shadow_definition.power = Some("13".to_string());
    shadow_definition.toughness = Some("13".to_string());
    shadow_definition.rules.push(shadow);
    let shadow = test_instance("shadow", shadow_definition, "player-0");
    engine.state.players[0].battlefield = vec![knight.clone(), shadow.clone()];
    engine.state.players[0].graveyard = vec![
        card("land-a", "Land", "player-0"),
        card("land-b", "Land", "player-0"),
        card("instant", "Instant", "player-0"),
    ];
    engine.state.players[0].life = 10;
    assert_eq!(engine.current_power(&knight), 4);
    assert_eq!(engine.current_toughness(&knight), 4);
    assert_eq!(engine.current_power(&shadow), 3);
    assert_eq!(engine.current_toughness(&shadow), 3);
    // The definition is recomputed when the life total changes (CR 611.3a).
    engine.state.players[0].life = 12;
    assert_eq!(engine.current_power(&shadow), 1);
}
