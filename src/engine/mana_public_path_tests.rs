//! Public-boundary proofs. Canonical mana IR is provisional until the separately
//! held parser emits the exact same rule; these tests do not claim card migration.
use super::*;

const POOL_ID: &str = "67f43ac6-2a58-4b53-b5d7-0330e2a252e2";
fn definition(
    id: &str,
    name: &str,
    type_line: &str,
    mana: &str,
    rules: Vec<Value>,
) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":name,"typeLine":type_line,"manaCost":mana,"rules":rules}),
    )
    .unwrap()
}
fn mana_rule(value: Value) -> Value {
    json!({"kind":"manaAbility","source":{"kind":"self"},"costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":value}]})
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Filler", "Land", "", vec![]);
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
        651,
    )
    .unwrap();
    e.state.step = GameStep::PrecombatMain;
    let input: Value =
        serde_json::from_str(include_str!("mana_public_reflecting_input.json")).unwrap();
    let real = &input[0];
    assert_eq!(real["oracle_id"], POOL_ID);
    assert_eq!(real["layout"], "normal");
    assert_eq!(
        real["oracle_text"],
        "{T}: Add one mana of any type that a land you control could produce."
    );
    let pool = definition(
        POOL_ID,
        real["name"].as_str().unwrap(),
        real["type_line"].as_str().unwrap(),
        "",
        vec![mana_rule(
            json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
        )],
    );
    assert!(rule_is_executable(&pool.rules[0]));
    e.state.players[0]
        .battlefield
        .push(card_instance("player-0", &pool, 0));
    e.record_event(
        "publicProofPrefix",
        None,
        None,
        json!({"sentinel":"CasePreserved"}),
    );
    e
}
fn anchor(e: &mut GameEngine, owner: usize, id: &str, symbol: &str, tapped: bool) {
    let def = definition(
        id,
        id,
        "Land",
        "",
        vec![mana_rule(json!(format!("{{{symbol}}}")))],
    );
    assert!(rule_is_executable(&def.rules[0]));
    let mut card = card_instance(&format!("player-{owner}"), &def, 0);
    card.tapped = tapped;
    e.state.players[owner].battlefield.push(card);
}
fn activation(symbols: &[&str]) -> CanonicalManaAbilityActivation {
    CanonicalManaAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: POOL_ID.into(),
        mana_symbols: Some(symbols.iter().map(|s| (*s).to_owned()).collect()),
    }
}
struct NoChoices {
    calls: usize,
}
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Err(EngineError::new("unexpected public mana callback"))
    }
    fn choose_number(
        &mut self,
        _: &GameState,
        _: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        self.calls += 1;
        Err(EngineError::new("unexpected public number callback"))
    }
}
fn unsupported_land(e: &mut GameEngine) {
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},"costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1},
        {"kind":"addMana","player":{"kind":"abilityController"},"mana":"{G}"}]});
    assert!(
        rule_is_executable(&rule),
        "real supported IR, unsupported projection shape"
    );
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition(
            "unsupported-prefix",
            "Generic stateful producer",
            "Land",
            "",
            vec![rule],
        ),
        0,
    ));
}
fn spell(e: &mut GameEngine) {
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":4}]});
    assert!(rule_is_executable(&rule));
    e.state.players[0].hand.push(card_instance(
        "player-0",
        &definition(
            "paid-spell",
            "Generic payment proof",
            "Sorcery",
            "{G}",
            vec![rule],
        ),
        0,
    ));
    e.admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
}

fn cast_request() -> CanonicalSpellCast {
    CanonicalSpellCast {
        controller_id: "player-0".into(),
        source_definition_id: "paid-spell".into(),
        use_alternative_cost: false,
        alternative_exile_definition_id: None,
        declaration_decisions: BTreeMap::new(),
    }
}

#[test]
fn public_pool_availability_then_activation_preserves_green_and_colorless() {
    for symbol in ["G", "C"] {
        let mut e = engine();
        anchor(&mut e, 0, "anchor", symbol, true);
        let request = activation(&[symbol]);
        assert!(e.canonical_mana_ability_available(&request).unwrap());
        let mut choices = NoChoices { calls: 0 };
        e.activate_canonical_mana_ability(&request, &mut choices)
            .unwrap();
        assert_eq!(choices.calls, 0);
        assert!(e.state.players[0].battlefield[0].tapped);
        assert_eq!(
            e.state.players[0]
                .mana_pool
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            [symbol]
        );
        let events = e
            .state
            .events
            .iter()
            .filter(|event| {
                event.kind == "manaAbilityActivated"
                    && event.card_instance_id.as_deref()
                        == Some(e.state.players[0].battlefield[0].instance_id.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].detail["mana"], json!([symbol]));
    }
}

#[test]
fn public_unanchored_cycle_is_a_legal_activation_without_bonus_bootstrap() {
    let mut e = engine();
    let sibling = definition(
        "query-sibling",
        "Generic query sibling",
        "Land",
        "",
        vec![mana_rule(
            json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
        )],
    );
    e.state.players[0]
        .battlefield
        .push(card_instance("player-0", &sibling, 0));
    e.state.rule_modifiers.push(
        json!({"kind":"additionalManaOnLandTap","where":{"kind":"cardTypeContains","value":"Land"},
        "playerId":"player-0","mana":"G","expiresAfterTurn":e.state.turn_number}),
    );
    let request = activation(&[]);
    assert!(e.canonical_mana_ability_available(&request).unwrap());
    let mut choices = NoChoices { calls: 0 };
    e.activate_canonical_mana_ability(&request, &mut choices)
        .unwrap();
    assert_eq!(choices.calls, 0);
    assert!(e.state.players[0].battlefield[0].tapped);
    assert!(!e.state.players[0].battlefield[1].tapped);
    assert!(e.state.players[0].mana_pool.is_empty());
    let activated = e
        .state
        .events
        .iter()
        .filter(|e| e.kind == "manaAbilityActivated")
        .collect::<Vec<_>>();
    assert_eq!(activated.len(), 1);
    assert_eq!(activated[0].detail["mana"], json!([]));
    assert!(!e.state.events.iter().any(|e| e.kind == "manaAdded"));
}

#[test]
fn public_query_uses_own_lands_not_opponent_anchors() {
    let mut e = engine();
    anchor(&mut e, 1, "opponent-red", "R", false);
    assert!(
        !e.canonical_mana_ability_available(&activation(&["R"]))
            .unwrap()
    );
    assert!(
        e.canonical_mana_ability_available(&activation(&[]))
            .unwrap()
    );
}

#[test]
fn public_unknown_context_is_an_error_before_availability_or_activation_mutates() {
    let mut e = engine();
    unsupported_land(&mut e);
    let before = serde_json::to_value(&e.state).unwrap();
    let failure = e
        .canonical_mana_ability_available(&activation(&["G"]))
        .unwrap_err();
    assert!(failure.message.contains("StatefulPrefix"));
    let mut choices = NoChoices { calls: 0 };
    let failure = e
        .activate_canonical_mana_ability(&activation(&["G"]), &mut choices)
        .unwrap_err();
    assert!(failure.message.contains("StatefulPrefix"));
    assert_eq!(choices.calls, 0);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn public_cast_uses_pool_as_automatic_source_then_resolves_real_spell() {
    let mut e = engine();
    anchor(&mut e, 0, "tapped-grounding-land", "G", true);
    spell(&mut e);
    assert!(e.canonical_spell_cast_available(&cast_request()).unwrap());
    let mut choices = NoChoices { calls: 0 };
    e.cast_canonical_spell(&cast_request(), &mut choices)
        .unwrap();
    assert_eq!(e.state.stack.len(), 1);
    assert!(e.state.players[0].hand.is_empty());
    assert!(e.state.players[0].battlefield[0].tapped);
    assert!(
        e.state
            .events
            .iter()
            .any(|event| event.kind == "manaAbilityActivated"
                && event.detail["mana"] == json!(["G"]))
    );
    e.resolve_canonical_top_stack(&mut choices).unwrap();
    assert_eq!(e.state.players[0].life, 24);
    assert!(e.state.players[0].mana_pool.is_empty());
    assert_eq!(choices.calls, 0);
}

#[test]
fn public_cast_unsupported_production_is_error_not_cannot_pay() {
    let mut e = engine();
    unsupported_land(&mut e);
    spell(&mut e);
    let before = serde_json::to_value(&e.state).unwrap();
    let failure = e
        .canonical_spell_cast_available(&cast_request())
        .unwrap_err();
    assert!(failure.message.contains("StatefulPrefix"));
    let mut choices = NoChoices { calls: 0 };
    assert!(
        e.cast_canonical_spell(&cast_request(), &mut choices)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(choices.calls, 0);
}

#[test]
fn stale_generated_mana_choice_is_revalidated_before_cost_or_history_changes() {
    let mut e = engine();
    anchor(&mut e, 0, "anchor", "G", true);
    let action = e
        .complete_checked_mana_activation_actions(0)
        .unwrap()
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref()
                == Some(e.state.players[0].battlefield[0].instance_id.as_str())
        })
        .unwrap();
    unsupported_land(&mut e);
    let before = serde_json::to_value(&e.state).unwrap();
    let mut choices = NoChoices { calls: 0 };
    let failure = e
        .apply_checked_priority_action(&action, &mut choices)
        .unwrap_err();
    assert!(failure.message.contains("StatefulPrefix"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn substitution_candidate_errors_propagate_and_zero_improvise_does_not_query() {
    let mut e = engine();
    e.state.players[0].battlefield.clear();
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition(
            "substitute",
            "Generic substitute",
            "Artifact Creature",
            "{G}",
            vec![],
        ),
        0,
    ));
    let mut choice = |_: &CardInstance, _: Option<&CardDefinition>| {
        Err::<Vec<ManaChoice>, _>(EngineError::new("candidate projection unknown"))
    };
    let convoke = e
        .try_plan_convoke_payment(
            0,
            &["{1}".into()],
            &BTreeSet::new(),
            None,
            None,
            1,
            &mut choice,
            &mut |_, _, _, _, _| Ok::<_, EngineError>(Some(ManaPayment::default())),
        )
        .unwrap_err();
    assert_eq!(convoke.message, "candidate projection unknown");
    let improvise = e
        .try_plan_improvise_reduction(0, &["{1}".into()], &BTreeSet::new(), None, 1, &mut choice)
        .unwrap_err();
    assert_eq!(improvise.message, "candidate projection unknown");
    let zero = e
        .try_plan_improvise_reduction(0, &["{1}".into()], &BTreeSet::new(), None, 0, &mut choice)
        .unwrap()
        .unwrap();
    assert_eq!(zero.0, ["{1}"]);
    assert!(zero.1.is_empty());
}

#[test]
fn stale_automatic_payment_is_refused_before_spell_movement_or_resource_cost() {
    let mut e = engine();
    anchor(&mut e, 0, "anchor", "G", true);
    spell(&mut e);
    let spell_id = e.state.players[0].hand[0].instance_id.clone();
    let action = e
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some(spell_id.as_str())
        })
        .unwrap();
    unsupported_land(&mut e);
    let before = serde_json::to_value(&e.state).unwrap();
    let mut choices = NoChoices { calls: 0 };
    let error = e
        .apply_checked_priority_action(&action, &mut choices)
        .unwrap_err();
    assert!(error.message.contains("StatefulPrefix"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(choices.calls, 0);
}

#[test]
fn convoke_remaining_payment_error_is_not_filtered_out_of_search() {
    let mut e = engine();
    e.state.players[0].battlefield.clear();
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition(
            "convoke-body",
            "Generic convoke body",
            "Creature",
            "{G}",
            vec![],
        ),
        0,
    ));
    let mut calls = 0;
    let error = e
        .try_plan_convoke_payment(
            0,
            &["{1}{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            1,
            &mut |_, _| Ok::<_, EngineError>(Vec::new()),
            &mut |_, _, _, _, _| {
                calls += 1;
                Err(EngineError::new("convoke remaining payment unknown"))
            },
        )
        .unwrap_err();
    assert_eq!(error.message, "convoke remaining payment unknown");
    assert_eq!(calls, 1);
}

#[test]
fn public_sacrifice_mana_without_tap_remains_legal_for_tapped_or_sick_sources() {
    for (type_line, tapped, sick) in [("Artifact", true, false), ("Creature", false, true)] {
        let mut e = engine();
        e.state.players[0].battlefield.clear();
        let mut rule = mana_rule(json!("{G}"));
        rule["costs"] = json!([{"kind":"sacrificePermanent","permanent":{"kind":"self"}}]);
        let mut def = definition(
            "no-tap-source",
            "Generic sacrifice producer",
            type_line,
            "",
            vec![rule],
        );
        def.power = Some("1".into());
        def.toughness = Some("1".into());
        let mut source = card_instance("player-0", &def, 0);
        source.tapped = tapped;
        source.summoning_sick = sick;
        e.state.players[0].battlefield.push(source);
        e.admit_explicit_test_fixture_objects()
            .expect("fixture construction precedes references");
        let request = CanonicalManaAbilityActivation {
            controller_id: "player-0".into(),
            source_definition_id: "no-tap-source".into(),
            mana_symbols: Some(vec!["G".into()]),
        };
        assert!(
            e.canonical_mana_ability_available(&request).unwrap(),
            "{type_line} tapped={tapped} sick={sick}"
        );
        let mut provider = NoChoices { calls: 0 };
        e.activate_canonical_mana_ability(&request, &mut provider)
            .unwrap();
        assert_eq!(provider.calls, 0);
        assert!(e.state.players[0].battlefield.is_empty());
        assert_eq!(e.state.players[0].graveyard.len(), 1);
        assert_eq!(
            e.state.players[0]
                .mana_pool
                .iter()
                .map(|mana| mana.symbol.as_str())
                .collect::<Vec<_>>(),
            ["G"]
        );
        assert_eq!(
            e.state
                .events
                .iter()
                .filter(|event| event.kind == "permanentTapped")
                .count(),
            0
        );
        assert_eq!(
            e.state
                .events
                .iter()
                .filter(|event| event.kind == "manaAbilityActivated")
                .count(),
            1
        );
    }
}
