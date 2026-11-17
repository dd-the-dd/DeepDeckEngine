use super::*;

fn position(count: usize) -> (GameEngine, StackObject, Vec<String>) {
    let definition: CardDefinition = serde_json::from_value(
        json!({"id":"filler","name":"Filler","typeLine":"Instant","manaCost":"{0}"}),
    )
    .unwrap();
    let engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); (count + 3).max(12)],
                })
                .collect(),
            opening_hand_size: 1,
            starting_player: 0,
        },
        1751,
    )
    .unwrap();
    let source = StackObject {
        id: "selection-frame".into(),
        controller: "p0".into(),
        card: engine.state.players[0].library[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let candidates = engine.state.players[0]
        .library
        .iter()
        .skip(1)
        .take(count)
        .map(|card| card.instance_id.clone())
        .collect();
    (engine, source, candidates)
}
struct Response {
    mode: usize,
    calls: usize,
    beyond_preview: bool,
}
impl DecisionProvider for Response {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "structured selection must use the existing card callback",
        ))
    }
    fn choose_card_instance_ids(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        self.calls += 1;
        let Some(DecisionChoice::CardSelection {
            candidate_card_instance_ids: cards,
            ..
        }) = &request.choice
        else {
            panic!("card domain")
        };
        let last = cards.last().unwrap().clone();
        self.beyond_preview = !request.options.iter().any(|option| {
            option.decisions["pick"]
                .as_array()
                .is_some_and(|ids| ids.contains(&Value::String(last.clone())))
        });
        Ok(match self.mode {
            0 => vec![last],
            1 => vec![cards[0].clone(), cards[0].clone()],
            2 => vec!["not-a-candidate".into()],
            3 => Vec::new(),
            _ => cards.clone(),
        })
    }
}
fn effect() -> Value {
    json!({"kind":"chooseCards","id":"pick","player":{"kind":"abilityController"},
        "from":{"kind":"boundObjects","binding":"view"},
        "minimum":{"kind":"integer","value":1},"maximum":{"kind":"integer","value":2}})
}

#[test]
fn structured_responses_are_validated_before_commit_and_not_limited_to_previews() {
    // This isolates the decision core; public look/move/rollback is covered by DATA.
    let (mut engine, source, objects) = position(3);
    let bindings = BTreeMap::from([("view".into(), RuntimeBinding::Objects(objects))]);
    for mode in 1..=4 {
        let before_events = engine.state.events.len();
        let mut decisions = BTreeMap::new();
        let mut provider = Response {
            mode,
            calls: 0,
            beyond_preview: false,
        };
        assert!(
            engine
                .execute_bound_card_selection(
                    &effect(),
                    None,
                    &source,
                    &bindings,
                    &mut decisions,
                    &mut provider
                )
                .is_err()
        );
        assert_eq!(provider.calls, 1);
        assert!(!decisions.contains_key("pick"));
        assert_eq!(engine.state.events.len(), before_events);
    }
    let (mut engine, source, objects) = position(130);
    let expected = objects.last().unwrap().clone();
    let bindings = BTreeMap::from([("view".into(), RuntimeBinding::Objects(objects))]);
    let mut choose = effect();
    choose["maximum"] = json!({"kind":"integer","value":1});
    let mut decisions = BTreeMap::new();
    let mut provider = Response {
        mode: 0,
        calls: 0,
        beyond_preview: false,
    };
    engine
        .execute_bound_card_selection(
            &choose,
            None,
            &source,
            &bindings,
            &mut decisions,
            &mut provider,
        )
        .unwrap();
    assert_eq!(provider.calls, 1);
    assert!(provider.beyond_preview);
    assert_eq!(decisions["pick"], json!([expected]));
}

#[test]
fn checked_cardinality_queries_preserve_real_empty_sets_and_prepare_empty_filters() {
    let (engine, source, objects) = position(3);
    let bindings = BTreeMap::from([
        ("view".into(), RuntimeBinding::Objects(objects)),
        ("empty".into(), RuntimeBinding::Objects(Vec::new())),
    ]);
    let view = json!({"kind":"boundObjects","binding":"view"});
    let empty = json!({"kind":"setDifference","left":view,"right":view});
    assert_eq!(
        engine
            .checked_runtime_object_count(&empty, &source, &bindings, &BTreeMap::new())
            .unwrap(),
        0
    );
    assert!(
        engine
            .checked_runtime_object_count(
                &json!({"kind":"boundObjects","binding":"missing"}),
                &source,
                &bindings,
                &BTreeMap::new()
            )
            .is_err()
    );
    let query = json!({"kind":"countObjects","objects":{"kind":"filterObjects","objects":{"kind":"boundObjects","binding":"empty"},"where":{"kind":"compare","operator":"<=","left":{"kind":"manaValueOf","object":{"kind":"candidate"}},"right":{"kind":"decisionResult","decisionId":"missingValue"}}}});
    assert!(
        engine
            .checked_runtime_numeric_expression(
                &query,
                &source,
                &bindings,
                &BTreeMap::new(),
                "test.quantity"
            )
            .is_err()
    );
    let from_hand = json!({"kind":"setDifference","left":{"kind":"cardsInZone","zone":{"kind":"hand","player":{"kind":"abilityController"}}},"right":{"kind":"boundObjects","binding":"empty"}});
    assert_eq!(
        engine
            .checked_runtime_object_count(&from_hand, &source, &bindings, &BTreeMap::new())
            .unwrap(),
        1
    );
}
