//! Real Oracle text -> primitive IR -> engine behavior for card operands in
//! zones (batch 12): a selection chosen from the collection its producer
//! restates (`the milled cards`, CR 608.2c), an outcome producer reading the
//! cards a leading sentence observed, a card exile linked to its source for
//! the source's other abilities (CR 607.2a), and the copy of that linked
//! card cast during resolution (CR 707.12, 707.10a).
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Zone Operand Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn rule_of_kind(rules: &[Value], kind: &str) -> Value {
    rules
        .iter()
        .find(|rule| rule["kind"].as_str() == Some(kind))
        .unwrap_or_else(|| panic!("a {kind} rule in {rules:#?}"))
        .clone()
}

fn stack_for(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: format!("stack:{}", source.instance_id),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: rule["kind"].as_str().map(str::to_string),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// An instant whose resolution gains its controller `life`.
fn life_instant(id: &str, life: i64) -> CardInstance {
    let mut definition = test_definition(id, "Instant");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": life },
        }],
    })];
    test_instance(id, definition, "player-0")
}

/// Accepts every permission and casts whenever a cast is offered; every
/// other request takes its first option.
struct AcceptAndCast;

impl DecisionProvider for AcceptAndCast {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(cast) = request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::CastSpell)
        {
            return Ok(cast);
        }
        if request.options.is_empty() {
            return Err(EngineError::new(format!("{} offers no option", request.id)));
        }
        // A permission is accepted: never the option that declines it.
        let declines = |action: &LegalAction| {
            action
                .decisions
                .values()
                .any(|value| *value == json!(["Decline"]) || *value == json!(false))
        };
        Ok(request
            .options
            .iter()
            .position(|action| !declines(action))
            .unwrap_or(0))
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn isochron_scepter_casts_a_copy_of_the_linked_card_that_leaves_no_card_behind() {
    let rules = parsed_rules(
        "Artifact",
        "Imprint \u{2014} When this artifact enters, you may exile an instant card with mana value 2 or less from your hand.\n{2}, {T}: You may copy the exiled card. If you do, you may cast the copy without paying its mana cost.",
    );
    // The hand exile records its link to the source (CR 607.2a).
    let trigger = rule_of_kind(&rules, "triggeredAbility");
    assert!(
        trigger.to_string().contains("\"linkToSource\":true"),
        "{trigger:#}"
    );
    let activated = rule_of_kind(&rules, "activatedAbility");
    assert!(
        activated.to_string().contains("createCardCopy"),
        "{activated:#}"
    );

    let mut engine = test_engine(2);
    let mut definition = test_definition("scepter", "Artifact");
    definition.rules = rules.clone();
    let scepter = test_instance("scepter", definition, "player-0");
    engine.state.players[0].battlefield.push(scepter.clone());
    engine.state.players[0].hand = vec![life_instant("imprinted-instant", 3)];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let life_before = engine.state.players[0].life;

    execute_effects_at_resolution_start(
        &mut engine,
        &trigger["effects"],
        &stack_for(&scepter, &trigger),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut AcceptAndCast,
    )
    .expect("the imprint trigger resolves");
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "imprinted-instant"),
        "the instant is exiled with the scepter"
    );

    execute_effects_at_resolution_start(
        &mut engine,
        &activated["effects"],
        &stack_for(&scepter, &activated),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut AcceptAndCast,
    )
    .expect("the copy ability resolves");
    assert_eq!(engine.state.stack.len(), 1, "the copy was cast");
    assert!(is_spell_copy(&engine.state.stack[0].card));
    engine
        .resolve_top_stack(&mut AcceptAndCast)
        .expect("the cast copy resolves");
    assert_eq!(engine.state.players[0].life, life_before + 3);
    // The copy ceased to exist when it left the stack (CR 707.10a): only the
    // imprinted card is in exile and nothing went to a graveyard.
    let exile = &engine.state.players[0].exile;
    assert_eq!(exile.len(), 1, "{exile:?}");
    assert_eq!(exile[0].instance_id, "imprinted-instant");
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .all(|card| !is_spell_copy(card))
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_linked_card_exile_records_its_source_and_an_unlinked_one_does_not() {
    let mut engine = test_engine(2);
    let source = test_instance("vault", test_definition("vault", "Artifact"), "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].hand = vec![
        life_instant("linked-card", 1),
        life_instant("unlinked-card", 1),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let exile = |binding: &str, link: bool| {
        let mut effect = json!({
            "kind": "moveCards",
            "cards": { "kind": "boundObjects", "binding": binding },
            "from": { "kind": "hand",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "to": { "kind": "exile",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "faceDown": false },
        });
        if link {
            effect["linkToSource"] = json!(true);
        }
        effect
    };
    let effects = json!([exile("linkedPick", true), exile("unlinkedPick", false)]);
    let mut bindings = BTreeMap::from([
        (
            "linkedPick".to_string(),
            RuntimeBinding::Objects(vec!["linked-card".to_string()]),
        ),
        (
            "unlinkedPick".to_string(),
            RuntimeBinding::Objects(vec!["unlinked-card".to_string()]),
        ),
    ]);
    let rule = json!({ "kind": "activatedAbility", "effects": effects });
    let linked = |engine: &GameEngine| {
        engine
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| modifier["kind"] == "exiledWithSource")
            .filter_map(|modifier| modifier["cardInstanceId"].as_str().map(str::to_string))
            .collect::<Vec<_>>()
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &effects,
        &stack_for(&source, &rule),
        &mut bindings,
        &mut BTreeMap::new(),
        &mut AcceptAndCast,
    )
    .expect("both cards are exiled");
    assert_eq!(engine.state.players[0].exile.len(), 2);
    assert_eq!(linked(&engine), vec!["linked-card".to_string()]);
    // `linkToSource` names a link only for an exile destination.
    for effect in effects.as_array().into_iter().flatten() {
        assert!(effect_supported(effect), "{effect}");
    }
    let mut to_hand = exile("linkedPick", true);
    to_hand["to"] = json!({ "kind": "hand",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } } });
    assert!(!effect_supported(&to_hand));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_selection_from_the_milled_cards_puts_exactly_one_matching_card_into_hand() {
    // Seed of Hope: `the milled cards` restate the mill (CR 608.2c); only
    // a permanent card among them is put into hand, then the life is gained.
    let rules = parsed_rules(
        "Instant",
        "Mill two cards. You may put a permanent card from among the milled cards into your hand. You gain 2 life.",
    );
    let spell = rule_of_kind(&rules, "spellAbility");
    let mut engine = test_engine(2);
    let library = &mut engine.state.players[0].library;
    library.clear();
    library.push(test_instance(
        "milled-creature",
        test_definition("milled-creature", "Creature \u{2014} Bear"),
        "player-0",
    ));
    library.push(life_instant("milled-instant", 1));
    let source = test_instance("seed", test_definition("seed", "Instant"), "player-0");
    let hand_before = engine.state.players[0].hand.len();
    let life_before = engine.state.players[0].life;
    execute_effects_at_resolution_start(
        &mut engine,
        &spell["effects"],
        &stack_for(&source, &spell),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut AcceptAndCast,
    )
    .expect("the spell resolves");
    let player = &engine.state.players[0];
    assert_eq!(player.hand.len(), hand_before + 1);
    assert!(
        player
            .hand
            .iter()
            .any(|card| card.instance_id == "milled-creature")
    );
    assert!(
        player
            .graveyard
            .iter()
            .any(|card| card.instance_id == "milled-instant"),
        "the instant is not a permanent card and stays milled"
    );
    assert_eq!(player.life, life_before + 2);
}
