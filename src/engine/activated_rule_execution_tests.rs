use super::*;

struct NoAdditionalChoices;

impl DecisionProvider for NoAdditionalChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "explicit activation requires no additional choices",
        ))
    }
}

fn test_definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": (type_line == "Creature").then_some("2"),
        "toughness": (type_line == "Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid")
}

fn test_instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn test_engine(player_count: usize) -> GameEngine {
    let filler = test_definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..player_count)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1471,
    )
    .expect("standalone test engine is valid")
}

#[test]
fn explicit_activation_x_selects_one_legal_declaration_without_hiding_ambiguity() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = (0..3)
        .map(|_| FloatingMana {
            symbol: "C".into(),
            spend_restriction: None,
        })
        .collect();
    let mut source = test_definition("variable-source", "Artifact");
    source.rules = vec![json!({
        "kind": "activatedAbility", "source": { "kind": "self" },
        "costs": [{ "kind": "payMana", "manaCost": "{X}" }],
        "declaration": { "kind": "castingDeclaration", "decisions": [
            { "kind": "chooseNumber", "id": "xValue", "minimum": 0 },
            { "kind": "chooseTargets", "id": "targetPermanent", "minimum": 1, "maximum": 1,
              "candidates": { "kind": "permanents", "where": { "kind": "and", "operands": [
                  { "kind": "cardTypeContains", "value": "Creature" },
                  { "kind": "compare", "operator": "<=",
                    "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
                    "right": { "kind": "decisionResult", "decisionId": "xValue" } }
              ] } } }
        ] },
        "effects": [{ "kind": "grantKeyword", "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                       "keyword": "cantBeBlocked", "duration": { "kind": "untilEndOfCurrentTurn" } }],
    })];
    assert!(rule_is_executable(&source.rules[0]));
    engine.state.players[0].battlefield = vec![
        test_instance("variable-source-instance", source, "player-0"),
        test_instance(
            "two-power-instance",
            test_definition("two-power", "Creature"),
            "player-0",
        ),
    ];
    let activation = CanonicalActivatedAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "variable-source".into(),
        ability_rule_index: Some(0),
        targets: BTreeMap::from([(
            "targetPermanent".into(),
            CanonicalTargetSelection::PermanentDefinition {
                definition_id: "two-power".into(),
            },
        )]),
    };
    assert!(
        engine
            .canonical_activated_ability_available(&activation)
            .is_err()
    );
    for (x, available) in [(1, false), (2, true), (3, true), (4, false)] {
        assert_eq!(
            engine.canonical_activated_ability_available_with_decisions(
                &activation,
                &BTreeMap::from([("xValue".into(), json!(x))]),
            ),
            Ok(available),
        );
    }
    assert_eq!(engine.state.players[0].mana_pool.len(), 3);
    assert!(engine.state.stack.is_empty());
    engine
        .activate_canonical_ability_with_decisions(
            &activation,
            &BTreeMap::from([("xValue".into(), json!(2))]),
            &mut NoAdditionalChoices,
        )
        .expect("explicit X selects one declaration and pays its exact cost");
    assert_eq!(engine.state.players[0].mana_pool.len(), 1);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].decisions["xValue"], 2);
}
