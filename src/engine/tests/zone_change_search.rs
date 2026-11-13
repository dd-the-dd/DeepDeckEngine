use super::*;

struct ZoneChangeSearchProvider;

impl DecisionProvider for ZoneChangeSearchProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.player_id, "player-1");
        let Some(DecisionChoice::OptionSelection { options, .. }) = request.choice.as_ref() else {
            return Err(EngineError::new("expected an optional-effect decision"));
        };
        options
            .iter()
            .position(|option| option == "Perform the optional effects")
            .ok_or_else(|| EngineError::new("missing optional-effect acceptance"))
    }

    fn choose_card_instance_ids(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        assert_eq!(request.player_id, "player-1");
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
                "unexpected library-search choice: {choice:?}"
            ))),
        }
    }
}

fn erode_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Erode".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(
            "Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    parsed.abilities[0]
        .rule
        .clone()
        .expect("native zone-change/search grammar should produce primitive IR")
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn destroy_search_uses_the_pre_destroy_controller_and_owner_graveyard() {
    let rule = erode_rule();
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
    ];

    let mut stack_object = StackObject {
        id: "stack:zone-change-search".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "zone-change-search",
            test_definition("zone-change-search", "Instant"),
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

    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut ZoneChangeSearchProvider,
        )
        .expect("primitive zone-change/search sequence should resolve");

    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .all(|permanent| { permanent.instance_id != "borrowed-creature" })
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| { card.instance_id == "borrowed-creature" })
    );
    let basic_land = engine.state.players[1]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "basic-land")
        .expect("the prior controller should receive the searched basic land");
    assert_eq!(basic_land.controller, "player-1");
    assert!(basic_land.tapped);
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "librarySearched" && event.player_id.as_deref() == Some("player-1")
    }));
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "zoneShuffled" && event.player_id.as_deref() == Some("player-1")
    }));
}
