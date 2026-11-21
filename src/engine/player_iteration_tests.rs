use super::*;
use serde_json::json;

#[derive(Default)]
struct Choices {
    players: Vec<String>,
    fail_at: Option<usize>,
}
impl DecisionProvider for Choices {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        if self.fail_at == Some(self.players.len()) {
            return Err(EngineError::new("synthetic player-loop choice refusal"));
        }
        Ok(0)
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn position(count: usize) -> (GameEngine, StackObject) {
    let filler = definition("filler", "Instant");
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..count)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        1891,
    )
    .unwrap();
    for index in 0..count {
        let id = format!("player-{index}");
        for sequence in 0..2 {
            engine.state.players[index].battlefield.push(card_instance(
                &id,
                &definition(&format!("candidate-{index}-{sequence}"), "Artifact"),
                sequence + 100,
            ));
        }
    }
    let source = card_instance("player-0", &definition("source", "Sorcery"), 55);
    let stack = StackObject {
        id: "synthetic-player-loop".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn player() -> Value {
    json!({"kind":"boundValue", "id":"player"})
}
fn choose() -> Value {
    json!({
        "kind":"chooseCards", "id":"chosen", "player":player(),
        "from":{"kind":"eachPermanent", "player":player()},
        "count":{"kind":"integer", "value":1},
    })
}
fn loop_effect(effects: Vec<Value>) -> Value {
    json!({"kind":"forEachPlayer", "players":{"kind":"eachPlayer"}, "bind":"player", "effects":effects})
}

#[test]
fn validates_arbitrary_children_and_typed_collection_boundary() {
    let mut effect = loop_effect(vec![
        json!({"kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":2}}),
        choose(),
    ]);
    effect["collect"] =
        json!({"bind":"selected", "objects":{"kind":"decisionResult","decisionId":"chosen"}});
    assert_eq!(player_iteration_supported(&effect), Some(true));
    assert!(effect_supported(&effect));
    let mut empty_bind = effect.clone();
    empty_bind["bind"] = json!("");
    for invalid in [
        empty_bind,
        loop_effect(vec![json!({"kind":"unknown-child"})]),
    ] {
        assert!(!effect_supported(&invalid));
    }
    let mut invalid = effect.clone();
    invalid["collect"]["bind"] = json!("player");
    assert!(!effect_supported(&invalid));
    invalid = effect.clone();
    invalid["collect"]["objects"] = json!({"kind":"unknown-collection"});
    assert!(!effect_supported(&invalid));
    invalid = effect.clone();
    invalid.as_object_mut().unwrap().remove("bind");
    // This is not the retained two-child historical template either.
    assert!(!effect_supported(&invalid));
}

#[test]
fn apnap_player_bindings_and_collection_have_lexical_scope() {
    let (mut engine, stack) = position(4);
    engine.state.active_player = 1;
    engine.state.players[2].has_lost = true;
    let hands = engine
        .state
        .players
        .iter()
        .map(|p| p.hand.len())
        .collect::<Vec<_>>();
    let mut effect = loop_effect(vec![
        json!({"kind":"bind", "id":"scratch", "value":{"kind":"integer","value":99}}),
        choose(),
        json!({"kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":2}}),
        json!({"kind":"drawCards", "player":player(), "count":{"kind":"integer","value":1}}),
    ]);
    effect["collect"] =
        json!({"bind":"selected", "objects":{"kind":"decisionResult","decisionId":"chosen"}});
    let mut bindings = BTreeMap::from([
        ("player".to_string(), RuntimeBinding::Integer(7)),
        ("scratch".to_string(), RuntimeBinding::Integer(8)),
    ]);
    let mut decisions = BTreeMap::from([("chosen".to_string(), json!(["outer-choice"]))]);
    let mut provider = Choices::default();
    assert!(
        engine
            .execute_player_iteration(
                &effect,
                &stack,
                &mut bindings,
                &mut decisions,
                &mut provider
            )
            .unwrap()
    );
    assert_eq!(provider.players, ["player-1", "player-3", "player-0"]);
    for (index, p) in engine.state.players.iter().enumerate() {
        assert_eq!(p.life, if index == 2 { 20 } else { 22 });
        assert_eq!(p.hand.len(), hands[index] + usize::from(index != 2));
    }
    let RuntimeBinding::Objects(selected) = &bindings["selected"] else {
        panic!("typed object collection");
    };
    let expected = [1, 3, 0]
        .into_iter()
        .map(|i| engine.state.players[i].battlefield[0].instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(selected, &expected);
    assert!(matches!(bindings["player"], RuntimeBinding::Integer(7)));
    assert!(matches!(bindings["scratch"], RuntimeBinding::Integer(8)));
    assert_eq!(decisions["chosen"], json!(["outer-choice"]));
    assert_eq!(bindings.len(), 3);
    assert_eq!(
        engine.quantity_snapshot_count(),
        0,
        "the loop creates no GameState checkpoint"
    );
}

#[test]
fn nested_loop_restores_the_outer_player_without_exporting_locals() {
    let (mut engine, stack) = position(3);
    let inner = json!({
        "kind":"forEachPlayer", "bind":"player",
        "players":{"kind":"opponentsOf", "player":player()},
        "effects":[{"kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":1}}],
    });
    let effect = loop_effect(vec![
        inner,
        json!({
            "kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":2},
        }),
    ]);
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    engine
        .execute_player_iteration(
            &effect,
            &stack,
            &mut bindings,
            &mut decisions,
            &mut Choices::default(),
        )
        .unwrap();
    assert!(engine.state.players.iter().all(|p| p.life == 24));
    assert!(bindings.is_empty());
    assert!(decisions.is_empty());
}

#[test]
fn child_error_restores_local_scope_and_reuses_an_explicit_outer_checkpoint() {
    let (mut engine, stack) = position(3);
    let effect = loop_effect(vec![
        json!({"kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":1}}),
        choose(),
    ]);
    let before = serde_json::to_value(&engine.state).unwrap();
    let mut bindings = BTreeMap::from([("player".to_string(), RuntimeBinding::Integer(7))]);
    let mut decisions = BTreeMap::from([("chosen".to_string(), json!(["outer"]))]);
    let mut provider = Choices {
        fail_at: Some(2),
        ..Choices::default()
    };
    let result = engine.with_replacement_checkpoint(true, |engine| {
        engine.execute_player_iteration(
            &effect,
            &stack,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
    });
    assert!(result.is_err());
    assert_eq!(provider.players, ["player-0", "player-1"]);
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert!(matches!(bindings["player"], RuntimeBinding::Integer(7)));
    assert_eq!(decisions["chosen"], json!(["outer"]));
    assert_eq!(engine.quantity_snapshot_count(), 1);
}

#[test]
fn unresolved_player_reference_is_not_treated_as_an_empty_set() {
    let (mut engine, stack) = position(2);
    let mut effect = loop_effect(vec![json!({
        "kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":2},
    })]);
    effect["players"] = json!({"kind":"boundValue", "id":"unbound"});
    assert!(effect_supported(&effect));
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    let result = engine.execute_player_iteration(
        &effect,
        &stack,
        &mut bindings,
        &mut decisions,
        &mut Choices::default(),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unresolved player reference")
    );
    assert!(bindings.is_empty());
    assert!(decisions.is_empty());
    assert!(engine.state.players.iter().all(|p| p.life == 20));
}

#[test]
fn mandatory_group_quantities_clip_to_availability_and_do_not_reselect_overlap() {
    let (mut engine, stack) = position(2);
    engine.state.players[0].battlefield[0].definition.type_line =
        "Artifact Creature - Construct".to_string();
    engine.state.players[0].battlefield[1].definition.type_line = "Creature - Scout".to_string();
    engine.state.players[1].battlefield.clear();
    let effect = json!({
        "kind":"forEachPlayer", "players":{"kind":"eachPlayer"}, "bind":"player",
        "effects":[
            {"kind":"bind", "id":"candidates", "value":{"kind":"eachPermanent", "player":player()}},
            {"kind":"chooseCards", "id":"artifact", "player":player(),
             "from":{"kind":"boundObjects", "binding":"candidates"},
             "where":{"kind":"cardTypeContains", "value":"Artifact"}, "count":{"kind":"integer", "value":2}},
            {"kind":"chooseCards", "id":"creature", "player":player(),
             "from":{"kind":"setDifference", "left":{"kind":"boundObjects", "binding":"candidates"},
                     "right":{"kind":"decisionResult", "decisionId":"artifact"}},
             "where":{"kind":"cardTypeContains", "value":"Creature"}, "count":{"kind":"integer", "value":2}}
        ],
        "collect":{"bind":"kept", "objects":{"kind":"union", "sets":[
            {"kind":"decisionResult", "decisionId":"artifact"},
            {"kind":"decisionResult", "decisionId":"creature"}
        ]}}
    });
    assert!(effect_supported(&effect));
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    let mut provider = Choices::default();
    engine
        .execute_player_iteration(
            &effect,
            &stack,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .unwrap();
    let RuntimeBinding::Objects(kept) = &bindings["kept"] else {
        panic!("kept collection");
    };
    assert_eq!(
        kept,
        &engine.state.players[0]
            .battlefield
            .iter()
            .map(|p| p.instance_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        provider.players.is_empty(),
        "clipped singleton and empty choices need no prompt"
    );
    assert_eq!(bindings.len(), 1);
    assert!(decisions.is_empty());
}

#[test]
fn empty_set_exports_empty_collection_and_random_set_selects_only_one_live_player() {
    let (mut engine, stack) = position(2);
    engine.state.players[1].has_lost = true;
    let mut effect = loop_effect(vec![
        json!({"kind":"bind", "id":"captured", "value":{"kind":"eachPermanent", "player":player()}}),
    ]);
    effect["players"] = json!({"kind":"opponentsOf", "player":{"kind":"abilityController"}});
    effect["collect"] =
        json!({"bind":"output", "objects":{"kind":"boundObjects","binding":"captured"}});
    let mut bindings = BTreeMap::from([(
        "output".to_string(),
        RuntimeBinding::Objects(vec!["stale".to_string()]),
    )]);
    let mut decisions = BTreeMap::new();
    engine
        .execute_player_iteration(
            &effect,
            &stack,
            &mut bindings,
            &mut decisions,
            &mut Choices::default(),
        )
        .unwrap();
    assert!(matches!(&bindings["output"], RuntimeBinding::Objects(objects) if objects.is_empty()));
    let random = json!({"kind":"forEachPlayer", "players":{"kind":"randomPlayer"}, "bind":"player", "effects":[
        {"kind":"gainLife", "player":player(), "amount":{"kind":"integer","value":1}}
    ]});
    assert!(effect_supported(&random));
    engine
        .execute_player_iteration(
            &random,
            &stack,
            &mut bindings,
            &mut decisions,
            &mut Choices::default(),
        )
        .unwrap();
    assert_eq!(engine.state.players[0].life, 21);
    assert_eq!(engine.state.players[1].life, 20);
}

#[test]
fn player_eliminated_by_an_earlier_iteration_is_not_visited_later() {
    let (mut engine, mut source) = position(3);
    source
        .decisions
        .insert("triggeringPlayerId".to_string(), json!("player-1"));
    let effect = loop_effect(vec![
        json!({"kind":"loseGame","player":{"kind":"triggeringPlayer"}}),
        json!({"kind":"addPlayerCounters","player":player(),"counter":"iteration","count":{"kind":"integer","value":1}}),
    ]);
    assert!(effect_supported(&effect));
    engine
        .execute_player_iteration(
            &effect,
            &source,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Choices::default(),
        )
        .unwrap();
    assert!(engine.state.players[1].has_lost);
    assert_eq!(
        engine.state.players[0]
            .counters
            .get("iteration")
            .copied()
            .unwrap_or(0),
        1
    );
    assert_eq!(
        engine.state.players[1]
            .counters
            .get("iteration")
            .copied()
            .unwrap_or(0),
        0
    );
    assert_eq!(
        engine.state.players[2]
            .counters
            .get("iteration")
            .copied()
            .unwrap_or(0),
        1
    );
}
