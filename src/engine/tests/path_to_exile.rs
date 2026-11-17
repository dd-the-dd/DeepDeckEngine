use super::*;

struct PathSearchProvider {
    accept_search: bool,
    card_selection_calls: usize,
}

impl DecisionProvider for PathSearchProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.player_id, "player-1");
        let expected_option = if self.accept_search {
            "Perform the optional effects"
        } else {
            "Decline"
        };
        let Some(DecisionChoice::OptionSelection { options, .. }) = request.choice.as_ref() else {
            return Err(EngineError::new("expected an optional-effect decision"));
        };
        options
            .iter()
            .position(|option| option == expected_option)
            .ok_or_else(|| EngineError::new(format!("missing {expected_option} option")))
    }

    fn choose_card_instance_ids(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        assert!(self.accept_search);
        assert_eq!(request.player_id, "player-1");
        self.card_selection_calls += 1;
        match request.choice.as_ref() {
            Some(DecisionChoice::CardSelection {
                candidate_card_instance_ids,
                minimum,
                maximum,
                ..
            }) => {
                assert_eq!((*minimum, *maximum), (0, 1));
                assert_eq!(candidate_card_instance_ids, &["basic-land".to_string()]);
                Ok(vec!["basic-land".to_string()])
            }
            choice => Err(EngineError::new(format!(
                "unexpected Path search choice: {choice:?}"
            ))),
        }
    }
}

fn path_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Path to Exile".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(
            "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    parsed.abilities[0]
        .rule
        .clone()
        .expect("Path to Exile should produce primitive IR")
}

fn resolve_path(accept_search: bool) -> (GameEngine, PathSearchProvider) {
    let rule = path_rule();
    assert!(rule_is_executable(&rule));

    let mut borrowed_creature = test_instance(
        "borrowed-creature",
        test_definition("borrowed-creature", "Creature"),
        "player-0",
    );
    borrowed_creature.controller = "player-1".to_string();

    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![borrowed_creature];
    engine.state.players[1].library = vec![
        test_instance(
            "nonbasic-land",
            test_definition("nonbasic-land", "Land"),
            "player-1",
        ),
        test_instance(
            "basic-land",
            test_definition("basic-land", "Basic Land - Forest"),
            "player-1",
        ),
        test_instance(
            "other-card",
            test_definition("other-card", "Sorcery"),
            "player-1",
        ),
    ];

    let mut stack_object = StackObject {
        id: "stack:path-to-exile".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "path-to-exile",
            test_definition("Path to Exile", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    stack_object.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "borrowed-creature".to_string(),
        },
    );
    let mut provider = PathSearchProvider {
        accept_search,
        card_selection_calls: 0,
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("Path to Exile's primitive sequence should resolve");
    (engine, provider)
}

fn assert_target_was_exiled_for_its_owner(engine: &GameEngine) {
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .all(|permanent| { permanent.instance_id != "borrowed-creature" })
    );
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| { card.instance_id == "borrowed-creature" })
    );
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .all(|card| { card.instance_id != "borrowed-creature" })
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn path_decline_still_exiles_but_does_not_search_or_shuffle() {
    let (engine, provider) = resolve_path(false);

    assert_target_was_exiled_for_its_owner(&engine);
    assert_eq!(provider.card_selection_calls, 0);
    assert_eq!(engine.state.players[1].library.len(), 3);
    assert!(
        engine
            .state
            .events
            .iter()
            .all(|event| { event.kind != "librarySearched" && event.kind != "zoneShuffled" })
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn path_accept_uses_the_pre_exile_controller_and_completes_the_search() {
    let (engine, provider) = resolve_path(true);

    assert_target_was_exiled_for_its_owner(&engine);
    assert_eq!(provider.card_selection_calls, 1);
    let basic_land = engine.state.players[1]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "basic-land")
        .expect("the target's controller should receive the searched basic land");
    assert_eq!(basic_land.controller, "player-1");
    assert!(basic_land.tapped);
    assert!(
        engine.state.players[1]
            .library
            .iter()
            .all(|card| card.instance_id != "basic-land")
    );

    let shuffled = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "zoneShuffled")
        .expect("the searched library should be shuffled");
    assert_eq!(shuffled.player_id.as_deref(), Some("player-1"));
    assert_eq!(shuffled.detail["zone"], "library");

    let searched = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "librarySearched")
        .expect("the search should be recorded");
    assert_eq!(searched.player_id.as_deref(), Some("player-1"));
    assert_eq!(searched.detail["cardIds"], json!(["basic-land"]));
}
