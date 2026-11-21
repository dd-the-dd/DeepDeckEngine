use super::*;
use rand::RngCore;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("unexpected query callback"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"rules":[]})).unwrap()
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Artifact");
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        9371,
    )
    .unwrap();
    engine.state.players[0].battlefield.push(card_instance(
        "p0",
        &definition("source", "Artifact"),
        50,
    ));
    engine
}

fn comparison(right: Value) -> Value {
    json!({"kind":"compare","left":{"kind":"manaValueOf","object":{"kind":"candidate"}},"operator":"<=","right":right})
}

fn execute(
    engine: &mut GameEngine,
    effects: Value,
    decisions: BTreeMap<String, Value>,
) -> Result<(), EngineError> {
    engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "source".into(),
                rule: json!({"kind":"spellAbility","source":{"kind":"self"},"effects":effects}),
                decisions,
                targets: BTreeMap::new(),
            },
            &mut NoChoices,
        )
        .map(|_| ())
}

#[test]
fn empty_collection_and_boolean_short_circuit_cannot_hide_numeric_failure() {
    for filter in [
        comparison(json!({"kind":"decisionResult","decisionId":"missing"})),
        json!({"kind":"or","operands":[null,comparison(json!({"kind":"decisionResult","decisionId":"missing"}))]}),
        json!({"kind":"not","operand":comparison(json!({"kind":"add","left":i32::MAX,"right":1}))}),
    ] {
        let mut engine = engine();
        let mut before = engine.clone();
        let rule = json!([
            {"kind":"gainLife","player":{"kind":"abilityController"},"amount":2},
            {"kind":"destroyPermanent","permanent":{"kind":"eachPermanent","where":filter}},
        ]);
        let error = execute(&mut engine, rule, BTreeMap::new()).unwrap_err();
        assert!(
            error.message.contains("objectQuery.compare.right"),
            "{}",
            error.message
        );
        assert_eq!(
            serde_json::to_value(&engine.state).unwrap(),
            serde_json::to_value(&before.state).unwrap()
        );
        assert_eq!(engine.rng.next_u64(), before.rng.next_u64());
        assert!(!engine.quantity_transaction_in_progress());
    }
}

#[test]
fn nontarget_query_uses_effective_types_and_ignores_phased_objects() {
    let mut engine = engine();
    let mut creature = definition("typed-land", "Land");
    creature.mana_cost = "{1}".into();
    let ordinary = card_instance("p1", &creature, 60);
    let mut phased = card_instance("p1", &definition("phased-creature", "Creature"), 61);
    phased.flags.insert("phasedOut".into(), true);
    engine.state.players[1]
        .battlefield
        .extend([ordinary, phased]);
    let mut lord = definition("type-lord", "Artifact");
    lord.rules.push(json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"addCardType","cardType":"Creature","objects":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Land"}}}]}));
    engine.state.players[1]
        .battlefield
        .push(card_instance("p1", &lord, 62));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let query = json!({"kind":"and","operands":[{"kind":"cardTypeContains","value":"Creature"},comparison(json!({"kind":"decisionResult","decisionId":"threshold"}))]});
    execute(
        &mut engine,
        json!([{"kind":"destroyPermanent","permanent":{"kind":"eachPermanent","where":query}}]),
        BTreeMap::from([("threshold".into(), json!(1))]),
    )
    .unwrap();
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|c| c.definition.id == "typed-land")
    );
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|c| c.definition.id == "phased-creature")
    );
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|c| c.definition.id == "type-lord")
    );
}

#[test]
fn empty_valid_collection_is_a_noop_and_no_numeric_target_declaration_is_created() {
    let mut engine = engine();
    let effects = json!([{"kind":"destroyPermanent","permanent":{"kind":"eachPermanent","where":{"kind":"and","operands":[{"kind":"cardTypeContains","value":"Creature"},comparison(json!(0))]}}}]);
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":effects});
    assert!(rule_is_executable(&rule));
    assert!(rule.get("declaration").is_none());
    execute(&mut engine, effects, BTreeMap::new()).unwrap();
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert!(engine.state.players[0].graveyard.is_empty());
}
