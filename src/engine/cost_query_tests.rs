use super::*;
fn definition(id: &str, line: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":line,"manaCost":"{3}","rules":[]}))
        .unwrap()
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land — Plains");
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        31519,
    )
    .unwrap()
}
fn query() -> Value {
    json!({"kind":"countObjects","objects":{"kind":"cardsInZone","zone":{"kind":"graveyard","player":{"kind":"abilityController"}},"where":{"kind":"or","operands":[{"kind":"cardTypeContains","value":"Instant"},{"kind":"cardTypeContains","value":"Sorcery"}]}}})
}

#[test]
fn exact_admitted_casting_incarnation_excludes_only_one_of_two_identical_definitions() {
    let mut e = engine();
    let spell = definition("same-printing", "Instant");
    e.state.players[0].graveyard.extend([
        card_instance("p0", &spell, 901),
        card_instance("p0", &spell, 902),
    ]);
    e.admit_explicit_test_fixture_objects().unwrap();
    let first = e.state.players[0].graveyard[0].clone();
    let second = e.state.players[0].graveyard[1].clone();
    let decisions = BTreeMap::new();
    let before = e.state.clone();
    for source in [&first, &second] {
        let context = CastingQueryContext::spell(&spell, "p0", &decisions, Some(source));
        assert_eq!(
            e.checked_cost_query_numeric(&query(), &context, "test.quantity")
                .unwrap(),
            1
        );
    }
    // Casting actor p1 is distinct from the physical source owner/controller
    // p0. Each explicit relationship keeps its established meaning.
    let foreign = CastingQueryContext::spell(&spell, "p1", &decisions, Some(&first));
    assert_eq!(
        e.checked_cost_query_numeric(&query(), &foreign, "test.quantity")
            .unwrap(),
        0
    );
    let mut owners = query();
    owners["objects"]["zone"]["player"] = json!({"kind":"ownerOf","object":{"kind":"self"}});
    assert_eq!(
        e.checked_cost_query_numeric(&owners, &foreign, "test.quantity")
            .unwrap(),
        1
    );
    let mut forged = first.clone();
    GameEngine::assign_tracked_zone_identity(
        &mut forged,
        GameEngine::tracked_zone_identity(&second).unwrap(),
    );
    let context = CastingQueryContext::spell(&spell, "p0", &decisions, Some(&forged));
    assert!(
        e.checked_cost_query_numeric(&query(), &context, "test.quantity")
            .unwrap_err()
            .message
            .contains("incarnation")
    );
    assert_eq!(e.state, before);
}

#[test]
fn definition_only_source_is_never_guessed_from_a_matching_printed_id_or_an_unproved_state_predicate()
 {
    let e = engine();
    let spell = definition("not-located", "Instant");
    let decisions = BTreeMap::new();
    let context = CastingQueryContext::spell(&spell, "p0", &decisions, None);
    assert!(
        e.checked_cost_query_numeric(&query(), &context, "test.quantity")
            .unwrap_err()
            .message
            .contains("casting source identity is required")
    );
    let creature = definition("known-disjoint", "Creature — Human");
    let context = CastingQueryContext::spell(&creature, "p0", &decisions, None);
    assert_eq!(
        e.checked_cost_query_numeric(&query(), &context, "test.quantity")
            .unwrap(),
        0
    );
    let mut unsupported = query();
    unsupported["objects"]["where"] = json!({"kind":"not","operand":{"kind":"isModified"}});
    assert!(
        e.checked_cost_query_numeric(&unsupported, &context, "test.quantity")
            .is_err()
    );
    let source = card_instance("p0", &creature, 999);
    let frame = StackObject {
        id: "runtime-query".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    assert!(
        e.resolve_runtime_object_ids_checked(
            &unsupported["objects"],
            &frame,
            &BTreeMap::new(),
            &BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn checked_constant_and_count_arithmetic_never_fall_back_to_zero_or_saturate() {
    let mut e = engine();
    let spell = definition("spell", "Creature — Human");
    e.state.players[0].graveyard.extend([
        card_instance("p0", &definition("one", "Instant"), 911),
        card_instance("p0", &definition("two", "Sorcery"), 912),
    ]);
    let decisions = BTreeMap::new();
    let context = CastingQueryContext::spell(&spell, "p0", &decisions, None);
    assert_eq!(
        e.checked_cost_query_numeric(
            &json!({"kind":"integer","value":2}),
            &context,
            "test.quantity"
        )
        .unwrap(),
        2
    );
    assert!(
        e.checked_cost_query_numeric(
            &json!({"kind":"multiply","left":query(),"right":i32::MAX}),
            &context,
            "test.quantity"
        )
        .is_err()
    );
    let mut invalid = query();
    invalid["objects"]["where"] = json!({"kind":"not","operand":{"kind":"compare","left":{"kind":"manaValueOf","object":{"kind":"candidate"}},"operator":"==","right":{"kind":"decisionResult","decisionId":"missing"}}});
    e.state.players[0].graveyard.clear();
    assert!(
        e.checked_cost_query_numeric(&invalid, &context, "test.quantity")
            .unwrap_err()
            .message
            .contains("objectQuery.compare.right")
    );
}

#[test]
fn reduction_applies_to_the_declared_player_population_but_condition_uses_source_actor() {
    let mut e = engine();
    let spell = definition("spell", "Instant");
    let mut reducer = definition("reducer", "Artifact");
    reducer.rules.push(json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{
        "kind":"reduceCastingCost","player":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}},
        "where":{"kind":"cardTypeContains","value":"Instant"},"amount":1,"condition":{"kind":"notYourTurn"}}]}));
    let mut source = card_instance("p1", &reducer, 920);
    source.controller = "p0".into();
    e.state.players[0].battlefield.push(source);
    assert!(rule_is_executable(&reducer.rules[0]));
    let decisions = BTreeMap::new();
    assert_eq!(
        e.checked_global_casting_cost_reduction("p1", &spell, "hand", &decisions, None)
            .unwrap(),
        0
    );
    e.state.active_player = 1;
    assert_eq!(
        e.checked_global_casting_cost_reduction("p1", &spell, "hand", &decisions, None)
            .unwrap(),
        1
    );
    assert_eq!(
        e.checked_global_casting_cost_reduction("p0", &spell, "hand", &decisions, None)
            .unwrap(),
        0
    );
}
