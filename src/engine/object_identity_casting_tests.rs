use super::*;

#[derive(Default)]
struct First {
    calls: usize,
}
impl DecisionProvider for First {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Ok(0)
    }
}
fn casting_engine() -> GameEngine {
    let creature = |id: &str| -> CardDefinition {
        serde_json::from_value(json!({"id":id,"name":id,"typeLine":"Creature","manaCost":"{0}","power":"2","toughness":"2","rules":[]})).unwrap()
    };
    let mut original: CardDefinition = serde_json::from_value(
        json!({"id":"original","name":"Original","typeLine":"Instant","manaCost":"{1}","rules":[]}),
    )
    .unwrap();
    original.rules = vec![
        json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"recipient","minimum":1,"maximum":1,"candidates":{"kind":"permanents","controller":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}},"where":{"kind":"cardTypeContains","value":"Creature"}}}]},"effects":[{"kind":"tapPermanent","permanent":{"kind":"chosenTarget","id":"recipient"}}]}),
    ];
    let mut copier = original.clone();
    copier.id = "copier".into();
    copier.name = "Copier".into();
    copier.mana_cost = "{0}".into();
    copier.rules = vec![
        json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"spell","minimum":1,"maximum":1,"candidates":{"kind":"spells"}}]},"effects":[{"kind":"copyStackItem","object":{"kind":"chosenTarget","id":"spell"},"controller":{"kind":"abilityController"},"mayChooseNewTargets":true}]}),
    ];
    let mut engine = GameEngine::new(
        GameSetup {
            players: vec![
                PlayerDeck {
                    id: "p0".into(),
                    name: "P0".into(),
                    starting_life: 20,
                    cards: vec![creature("friendly"), copier],
                },
                PlayerDeck {
                    id: "p1".into(),
                    name: "P1".into(),
                    starting_life: 20,
                    cards: vec![creature("opposing"), original],
                },
            ],
            opening_hand_size: 0,
            starting_player: 0,
        },
        17002,
    )
    .unwrap();
    engine
        .configure_deterministic_position(&DeterministicGamePosition {
            turn_number: 1,
            active_player_id: "p0".into(),
            step: GameStep::PrecombatMain,
            players: vec![
                DeterministicPlayerPosition {
                    player_id: "p0".into(),
                    life: 20,
                    battlefield_definition_ids: vec!["friendly".into()],
                    hand_definition_ids: vec!["copier".into()],
                    library_size: 0,
                    mana_pool: Vec::new(),
                },
                DeterministicPlayerPosition {
                    player_id: "p1".into(),
                    life: 20,
                    battlefield_definition_ids: vec!["opposing".into()],
                    hand_definition_ids: vec!["original".into()],
                    library_size: 0,
                    mana_pool: vec!["C".into()],
                },
            ],
        })
        .unwrap();
    engine
}
fn cast(
    engine: &mut GameEngine,
    actor: &str,
    id: &str,
    provider: &mut First,
) -> Result<(), EngineError> {
    engine.cast_canonical_spell(
        &CanonicalSpellCast {
            controller_id: actor.into(),
            source_definition_id: id.into(),
            use_alternative_cost: false,
            alternative_exile_definition_id: None,
            declaration_decisions: BTreeMap::new(),
        },
        provider,
    )
}
#[test]
fn public_cast_refuses_missing_designation_before_mana_or_stack_changes() {
    let mut engine = casting_engine();
    GameEngine::clear_tracked_zone_identity(&mut engine.state.players[0].battlefield[0]);
    let before = engine.state.clone();
    let rng = engine.rng_fingerprint_for_search();
    let sequence = engine.stack_sequence;
    let mut provider = First::default();
    let error = cast(&mut engine, "p1", "original", &mut provider).unwrap_err();
    assert!(error.to_string().contains("no admitted incarnation"));
    assert_eq!(engine.state, before);
    assert_eq!(engine.rng_fingerprint_for_search(), rng);
    assert_eq!(engine.stack_sequence, sequence);
    assert_eq!(provider.calls, 0);
    // Independent valid construction proves the spell/cost/timing setup itself.
    let mut valid = casting_engine();
    cast(&mut valid, "p1", "original", &mut First::default()).unwrap();
    assert_eq!(valid.state.stack.len(), 1);
    assert!(valid.state.players[1].mana_pool.is_empty());
}
#[test]
fn copy_retarget_capture_error_restores_created_copy_stack_ids_rng_and_events() {
    let mut engine = casting_engine();
    let mut provider = First::default();
    cast(&mut engine, "p1", "original", &mut provider).unwrap();
    cast(&mut engine, "p0", "copier", &mut provider).unwrap();
    assert_eq!(engine.state.stack.len(), 2);
    // Deliberately malformed carrier, not an unadmitted positive fixture.
    GameEngine::clear_tracked_zone_identity(&mut engine.state.players[1].battlefield[0]);
    let before = engine.state.clone();
    let rng = engine.rng_fingerprint_for_search();
    let sequence = engine.stack_sequence;
    let copies = engine.copy_sequence;
    let calls = provider.calls;
    let error = engine
        .resolve_canonical_top_stack(&mut provider)
        .unwrap_err();
    assert!(error.to_string().contains("no admitted incarnation"));
    assert_eq!(engine.state, before);
    assert_eq!(engine.rng_fingerprint_for_search(), rng);
    assert_eq!(engine.stack_sequence, sequence);
    assert_eq!(engine.copy_sequence, copies);
    assert_eq!(provider.calls, calls);
    assert!(
        !engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "stackObjectCopied")
    );
}
