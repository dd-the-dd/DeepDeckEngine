use super::super::*;

struct StartOnTwo;
impl DecisionProvider for StartOnTwo {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        request
            .options
            .iter()
            .position(|option| option.decisions.get("readAheadChapter") == Some(&json!(2)))
            .ok_or_else(|| EngineError::new("unexpected saga quantity choice"))
    }
}
fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"manaCost":"{1}"})).unwrap()
}
fn chapters(engine: &GameEngine) -> Vec<i32> {
    engine
        .state
        .stack
        .iter()
        .filter_map(|object| object.ability_rule.as_ref())
        .flat_map(|rule| rule["event"]["chapters"].as_array().into_iter().flatten())
        .filter_map(integer_value)
        .collect()
}
#[test]
fn read_ahead_restricts_later_doubled_lore_only_during_the_entry_turn() {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player{n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Land"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        705,
    )
    .unwrap();
    let mut saga = card_instance("player-0", &definition("saga", "Enchantment - Saga"), 1);
    saga.definition.rules = vec![
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"readAhead"}}),
    ];
    saga.definition.rules.extend([2,3,4,6].into_iter().map(|n|json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"sagaChapterReached","object":{"kind":"self"},"chapters":[{"kind":"integer","value":n}]},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]})));
    let mut chooser = StartOnTwo;
    engine
        .apply_enter_replacements(&mut saga, &mut chooser)
        .unwrap();
    assert_eq!(saga.counters.get("lore"), Some(&2));
    let id = saga.instance_id.clone();
    engine.commit_prepared_battlefield_entries(vec![(0, saga)]);
    assert_eq!(chapters(&engine), vec![2]);
    engine.resolve_top_stack(&mut chooser).unwrap();
    let mut season = card_instance("player-0", &definition("season", "Enchantment"), 2);
    season.definition.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"replaceEventQuantity","event":{"kind":"counterPlacement","recipientKind":"permanent","origin":"effect","minimum":1,"player":{"kind":"controllerOf","object":{"kind":"self"}}},"quantity":{"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}}]}),
    ];
    engine.state.players[0].battlefield.push(season);
    engine
        .place_counters(
            &id,
            "lore",
            1,
            QuantityOrigin::Effect,
            Some("player-0"),
            &mut chooser,
        )
        .unwrap();
    assert_eq!(chapters(&engine), vec![4]);
    let (p, c) = engine.permanent_position(&id).unwrap();
    assert_eq!(
        engine.state.players[p].battlefield[c].counters.get("lore"),
        Some(&4)
    );
    engine.resolve_top_stack(&mut chooser).unwrap();
    engine.remove_counters_from_permanent(&id, "lore", 2, "effect");
    engine.cleanup(&mut chooser).unwrap();
    engine.state.turn_number += 1;
    engine.state.active_player = 1;
    engine.start_turn(&mut chooser).unwrap();
    let (p, c) = engine.permanent_position(&id).unwrap();
    assert!(
        !engine.state.players[p].battlefield[c]
            .flags
            .get("enteredThisTurn")
            .copied()
            .unwrap_or(false)
    );
    engine
        .place_counters(
            &id,
            "lore",
            1,
            QuantityOrigin::Effect,
            Some("player-0"),
            &mut chooser,
        )
        .unwrap();
    assert_eq!(chapters(&engine), vec![3, 4]);
}
