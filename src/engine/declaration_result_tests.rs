use super::*;

fn definition(id: &str, type_line: &str, mana_cost: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":type_line,"manaCost":mana_cost,"rules":rules}),
    )
    .unwrap()
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Land", "", vec![]);
    let mut e = GameEngine::new(
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
        649,
    )
    .unwrap();
    for i in 0..3 {
        e.state.players[0].battlefield.push(card_instance(
            "player-0",
            &definition(&format!("forest-{i}"), "Basic Land — Forest", "", vec![]),
            0,
        ));
    }
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("executor", "Artifact", "", vec![]),
        0,
    ));
    e
}
fn spell_rule() -> Value {
    json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]})
}
fn snapshot(declarations: &[CastingDeclaration]) -> Value {
    Value::Array(
        declarations
            .iter()
            .map(|d| {
                json!({"decisions":d.decisions,
        "targets":d.targets,"targetOrder":d.target_order})
            })
            .collect(),
    )
}

#[test]
fn implicit_spell_x_preserves_exact_values_order_and_metadata() {
    let e = engine();
    let card = definition("spell-x", "Instant", "{X}{G}", vec![spell_rule()]);
    let old = e.declarations_for_card(&card, "player-0").unwrap();
    let mut requests = 0;
    let checked = e
        .try_declarations_for_card_with_budget(&card, "player-0", &mut |controller, spending| {
            requests += 1;
            assert_eq!(controller, "player-0");
            assert_eq!(spending.unwrap().id, "spell-x");
            Ok::<_, EngineError>(3)
        })
        .unwrap()
        .unwrap();
    assert_eq!(requests, 1);
    assert_eq!(snapshot(&checked), snapshot(&old));
    assert_eq!(
        checked
            .iter()
            .map(|d| d.decisions["xValue"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
}

#[test]
fn explicit_number_maximum_wins_and_missing_maximum_preserves_budget_error() {
    let e = engine();
    let mut rule = spell_rule();
    rule["declaration"] = json!({"kind":"castingDeclaration","decisions":[{"kind":"chooseNumber","id":"xValue","minimum":0,"maximum":2}]});
    assert!(declaration_shape_supported(&rule));
    let mut card = definition("explicit-x", "Instant", "", vec![rule]);
    let mut calls = 0;
    let declared = e
        .try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| {
            calls += 1;
            Err::<usize, _>(EngineError::new("must not query an explicit maximum"))
        })
        .unwrap()
        .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(declared.len(), 3);
    assert_eq!(
        snapshot(&declared),
        snapshot(&e.declarations_for_card(&card, "player-0").unwrap())
    );
    card.rules[0]["declaration"]["decisions"][0]
        .as_object_mut()
        .unwrap()
        .remove("maximum");
    let failure = e
        .try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| {
            Err::<usize, _>(EngineError::new("exact source-budget rejection"))
        })
        .unwrap_err();
    assert_eq!(
        failure.into_engine_error().message,
        "exact source-budget rejection"
    );
}

#[test]
fn implicit_activated_x_keeps_old_one_through_maximum_sequence() {
    let e = engine();
    let source = &e.state.players[0].battlefield[3];
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}},{"kind":"payMana","manaCost":"{X}"}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":"{G}"}]});
    let old = e
        .declarations_for_activated_rule(&rule, "player-0", &source.instance_id)
        .unwrap();
    let mut calls = 0;
    let declared = e
        .try_declarations_for_activated_rule_with_budget(
            &rule,
            "player-0",
            &source.instance_id,
            &mut |controller, spending| {
                calls += 1;
                assert_eq!(controller, "player-0");
                assert!(spending.is_none());
                Ok::<_, EngineError>(3)
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(snapshot(&old), snapshot(&declared));
    assert_eq!(
        declared
            .iter()
            .map(|d| d.decisions["xValue"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let error = e
        .try_declarations_for_activated_rule_with_budget(
            &rule,
            "player-0",
            &source.instance_id,
            &mut |_, _| Err::<usize, _>(EngineError::new("activated budget unavailable")),
        )
        .unwrap_err();
    assert_eq!(
        error.into_engine_error().message,
        "activated budget unavailable"
    );
}

#[test]
fn declaration_grammar_rejection_stays_none_and_never_masquerades_as_budget_error() {
    let e = engine();
    let mut rule = spell_rule();
    rule["declaration"] = json!({"kind":"castingDeclaration","decisions":[{"kind":"chooseNumber","id":"xValue","minimum":0}]});
    assert!(
        declaration_shape_supported(&rule),
        "positive sibling schema must be valid"
    );
    rule["declaration"]["decisions"][0]["minimum"] = json!(-1);
    assert!(
        !declaration_shape_supported(&rule),
        "only negative minimum causes the near-miss"
    );
    let card = definition("invalid", "Instant", "", vec![rule]);
    let mut calls = 0;
    let value = e
        .try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| {
            calls += 1;
            Err::<usize, _>(EngineError::new("must not be reached"))
        })
        .unwrap();
    assert!(value.is_none());
    assert!(e.declarations_for_card(&card, "player-0").is_none());
    assert_eq!(calls, 0);
}

#[test]
fn complete_checked_generator_uses_real_declaration_reader_for_fixed_and_no_mana() {
    let mut e = engine();
    e.state.players[0].battlefield.clear();
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":"{G}"}]});
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("producer", "Land", "", vec![rule]),
        0,
    ));
    let fixed = e.complete_checked_mana_activation_actions(0).unwrap();
    assert_eq!(fixed.len(), 1);
    assert_eq!(fixed[0].decisions["manaProduced"], json!(["G"]));
    e.state.players[0].battlefield[0].definition.rules[0]["effects"][0]["mana"] =
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1});
    let before = serde_json::to_value(&e.state).unwrap();
    let undefined = e.complete_checked_mana_activation_actions(0).unwrap();
    assert_eq!(undefined.len(), 1);
    assert_eq!(undefined[0].decisions["manaProduced"], json!([]));
    assert_eq!(undefined[0].decisions["tapSource"], json!(true));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    // A complete private discovery chain, still no claim of public activation.
}

#[test]
fn generic_budget_failure_and_capture_failure_preserve_their_distinct_channels() {
    use super::DeclarationDependencyFailure;
    let mut e = engine();
    let mut rule = spell_rule();
    rule["declaration"] = json!({"kind":"castingDeclaration","decisions":[{
        "kind":"chooseTargets","id":"land","minimum":1,"maximum":1,
        "candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Land"}}
    }]});
    let card = definition("targeted-x", "Instant", "{X}", vec![rule]);
    let before = e.state.clone();
    let budget = e.try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| {
        Err::<usize, _>("budget sentinel")
    });
    assert!(matches!(
        budget,
        Err(DeclarationDependencyFailure::Dependency("budget sentinel"))
    ));
    // Deliberately unadmitted fixture objects: the budget can be valid while
    // designation authentication fails. No world mutation is allowed.
    let capture =
        e.try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| Ok::<_, &str>(0));
    match capture {
        Err(DeclarationDependencyFailure::Capture(error)) => {
            assert!(error.message.contains("no admitted incarnation"));
        }
        _ => panic!("capture failure must retain its own error channel"),
    }
    assert_eq!(e.state, before);
    e.admit_explicit_test_fixture_objects().unwrap();
    let valid = e
        .try_declarations_for_card_with_budget(&card, "player-0", &mut |_, _| Ok::<_, &str>(0))
        .unwrap()
        .unwrap();
    assert_eq!(valid.len(), 3);
}
