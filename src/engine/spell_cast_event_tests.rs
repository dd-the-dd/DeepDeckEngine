use super::*;

struct RefuseChoices;
impl DecisionProvider for RefuseChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "this event scenario needs no implicit choice",
        ))
    }
}

fn definition(id: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":"Instant","manaCost":"{0}","rules":rules}),
    )
    .unwrap()
}

fn engine() -> GameEngine {
    let filler = definition("filler", vec![]);
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1802,
    )
    .unwrap();
    let watcher_rule = json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"spellCast","anyPlayer":true,"where":null,"spellCastOrdinal":2},"effects":[{"kind":"drawCards","player":{"kind":"controllerOf","object":{"kind":"self"}},"count":{"kind":"integer","value":1}}]});
    assert!(rule_is_executable(&watcher_rule));
    let mut watcher = definition("observer", vec![watcher_rule]);
    watcher.type_line = "Artifact".to_string();
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &watcher, 1803));
    for n in 0..2 {
        for i in 0..4 {
            let id = format!("p{n}-spell-{i}");
            let spell = definition(
                &id,
                vec![json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[]})],
            );
            engine.state.players[n].hand.push(card_instance(
                &format!("player-{n}"),
                &spell,
                1810 + n * 4 + i,
            ));
        }
    }
    engine.state.step = GameStep::PrecombatMain;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine
}

fn draw_count(engine: &GameEngine) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "cardDrawn" && event.player_id.as_deref() == Some("player-0"))
        .count()
}

fn cast_and_resolve(engine: &mut GameEngine, caster: usize, index: usize) {
    engine
        .cast_canonical_spell(
            &CanonicalSpellCast {
                controller_id: format!("player-{caster}"),
                source_definition_id: format!("p{caster}-spell-{index}"),
                use_alternative_cost: false,
                alternative_exile_definition_id: None,
                declaration_decisions: BTreeMap::new(),
            },
            &mut RefuseChoices,
        )
        .unwrap();
    while !engine.state.stack.is_empty() {
        engine.resolve_top_stack(&mut RefuseChoices).unwrap();
    }
}

#[test]
fn second_cast_ordinal_is_independent_per_caster_and_resets_after_the_real_turn_boundary() {
    let mut engine = engine();
    cast_and_resolve(&mut engine, 0, 0);
    cast_and_resolve(&mut engine, 1, 0);
    assert_eq!(draw_count(&engine), 0);
    cast_and_resolve(&mut engine, 0, 1);
    cast_and_resolve(&mut engine, 1, 1);
    assert_eq!(draw_count(&engine), 2);
    let first_turn = engine.state.turn_number;
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "spellCast" && event.turn_number == first_turn)
            .count(),
        4
    );

    engine.cleanup(&mut RefuseChoices).unwrap();
    engine.state.turn_number += 1;
    engine.state.active_player = 1;
    engine.start_turn(&mut RefuseChoices).unwrap();
    engine.transition_to_step(GameStep::PrecombatMain);
    // Do not clear or reconstruct the event log: past casts remain available,
    // and the existing turn-qualified query must exclude them by provenance.
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "spellCast" && event.turn_number == first_turn)
            .count(),
        4
    );
    cast_and_resolve(&mut engine, 1, 2);
    cast_and_resolve(&mut engine, 0, 2);
    assert_eq!(draw_count(&engine), 2);
    cast_and_resolve(&mut engine, 1, 3);
    cast_and_resolve(&mut engine, 0, 3);
    assert_eq!(draw_count(&engine), 4);
    assert!(engine.state.stack.is_empty());
}
