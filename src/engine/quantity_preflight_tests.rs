use super::super::*;
use super::{definition_may_enable_quantity, payload_risk};

struct RefuseOrder {
    calls: usize,
}
impl DecisionProvider for RefuseOrder {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        assert_eq!(request.player_id, "player-0");
        self.calls += 1;
        Err(EngineError::new("late quantity order refused"))
    }
}

struct FirstOrder;
impl DecisionProvider for FirstOrder {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        Ok(0)
    }
}

fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"manaCost":"{1}"})).unwrap()
}

fn engine() -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Basic Land - Island"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        633,
    )
    .unwrap();
    engine.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("executor", "Artifact"),
        701,
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine
}

fn source_rule() -> Value {
    let event = json!({"kind":"tokenCreation","origin":"effect","minimum":1,"player":{"kind":"controllerOf","object":{"kind":"self"}}});
    json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[
        {"kind":"replaceEventQuantity","event":event,"quantity":{"kind":"add","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":1}}},
        {"kind":"replaceEventQuantity","event":event,"quantity":{"kind":"multiply","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":2}}}
    ]})
}

fn create_ordinary(amount: i32) -> Value {
    json!({"kind":"createTokens","controller":{"kind":"abilityController"},"quantity":{"kind":"integer","value":amount},"token":{"name":"Ordinary","types":["Creature"],"power":1,"toughness":1}})
}

fn execution(effects: Vec<Value>, targets: BTreeMap<String, TargetRef>) -> CanonicalRuleExecution {
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":effects});
    assert!(rule_is_executable(&rule));
    CanonicalRuleExecution {
        controller_id: "player-0".to_string(),
        source_definition_id: "executor".to_string(),
        rule,
        decisions: BTreeMap::new(),
        targets,
    }
}

fn assert_late_refusal_restores_all(engine: &mut GameEngine, execution: CanonicalRuleExecution) {
    let before = engine.state.clone();
    let branches = engine.branching_decision_sequence.get();
    let mut provider = RefuseOrder { calls: 0 };
    let error = engine
        .execute_canonical_rule(execution, &mut provider)
        .unwrap_err();
    assert!(error.to_string().contains("late quantity order refused"));
    assert_eq!(provider.calls, 1);
    assert_eq!(engine.state, before);
    assert_eq!(engine.branching_decision_sequence.get(), branches);
    assert_eq!(engine.quantity_snapshot_count(), 1);
    assert_eq!(engine.quantity_preflight_count(), 1);
    assert!(!engine.quantity_transaction_in_progress());
    assert!(!engine.quantity_preflight_in_progress());
}

#[test]
fn inline_source_introduced_before_late_failure_is_rolled_back_and_retry_is_clean() {
    let mut engine = engine();
    assert!(!engine.quantity_replacements_active());
    let create_source = json!({"kind":"createTokens","controller":{"kind":"abilityController"},"quantity":{"kind":"integer","value":1},"token":{"name":"Quantity Source","types":["Enchantment","Creature"],"power":1,"toughness":1,"abilities":[source_rule()]}});
    let program = execution(vec![create_source, create_ordinary(1)], BTreeMap::new());
    assert_late_refusal_restores_all(&mut engine, program.clone());
    engine
        .execute_canonical_rule(program, &mut FirstOrder)
        .unwrap();
    assert_eq!(engine.quantity_snapshot_count(), 2);
    assert_eq!(engine.quantity_preflight_count(), 2);
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.definition.is_token)
            .count(),
        5
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "tokenCreated")
            .count(),
        5
    );
}

fn put_source_in_graveyard(engine: &mut GameEngine) -> String {
    let mut source = card_instance(
        "player-0",
        &definition("quantity-source", "Enchantment"),
        702,
    );
    source.definition.rules = vec![source_rule()];
    let id = source.instance_id.clone();
    engine.state.players[0].graveyard.push(source);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    id
}

#[test]
fn source_moved_from_known_zone_before_late_failure_restores_zone_and_entry_events() {
    let mut engine = engine();
    let id = put_source_in_graveyard(&mut engine);
    assert!(!engine.quantity_replacements_active());
    let movement = json!({"kind":"moveCards","cards":{"kind":"chosenTargets","id":"incoming"},"to":{"kind":"battlefield","player":{"kind":"abilityController"},"tapped":false}});
    let program = execution(
        vec![movement, create_ordinary(1)],
        BTreeMap::from([("incoming".to_string(), TargetRef::Card { instance_id: id })]),
    );
    assert_late_refusal_restores_all(&mut engine, program);
}

#[test]
fn copying_a_known_definition_before_late_failure_restores_original_definition() {
    let mut engine = engine();
    let id = put_source_in_graveyard(&mut engine);
    assert!(!engine.quantity_replacements_active());
    // The known template enters through the ordinary movement service first.
    // Capture its NEW object through the movement output rather than following
    // an old graveyard target across its zone change. The rollback proof still
    // includes a real copy transition before the provider refuses the ordering.
    let enter = json!({"kind":"moveCards","cards":{"kind":"chosenTargets","id":"template"},
        "to":{"kind":"battlefield","player":{"kind":"abilityController"},"tapped":false},"bind":"enteredTemplate"});
    let copy = json!({"kind":"forEachObject","objects":{"kind":"boundObjects","binding":"enteredTemplate"},"bind":"copyTemplateObject",
        "effects":[{"kind":"becomeCopyOfPermanent","object":{"kind":"self"},"copy":{"kind":"boundValue","id":"copyTemplateObject"},
            "retainResolvingAbility":false,"duration":{"kind":"untilEndOfCurrentTurn"}}]});
    let program = execution(
        vec![enter, copy, create_ordinary(1)],
        BTreeMap::from([("template".to_string(), TargetRef::Card { instance_id: id })]),
    );
    assert_late_refusal_restores_all(&mut engine, program);
}

#[test]
fn an_inactive_known_source_is_included_before_its_condition_becomes_true() {
    let mut engine = engine();
    let mut source = card_instance(
        "player-0",
        &definition("conditional-source", "Enchantment"),
        703,
    );
    let mut rule = source_rule();
    rule["condition"] =
        json!({"kind":"controlsPermanent","where":{"kind":"cardTypeContains","value":"Creature"}});
    source.definition.rules = vec![rule];
    engine.state.players[0].battlefield.push(source);
    assert!(!engine.quantity_replacements_active());
    let program = execution(
        vec![create_ordinary(1), create_ordinary(1)],
        BTreeMap::new(),
    );
    assert_late_refusal_restores_all(&mut engine, program);
}

#[test]
fn ordinary_hundred_token_program_has_one_preflight_and_no_checkpoint() {
    let mut engine = engine();
    let program = execution(vec![create_ordinary(100)], BTreeMap::new());
    engine
        .execute_canonical_rule(program, &mut RefuseOrder { calls: 0 })
        .unwrap();
    assert_eq!(engine.quantity_snapshot_count(), 0);
    assert_eq!(engine.quantity_preflight_count(), 1);
    assert!(!engine.quantity_preflight_in_progress());
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.definition.is_token)
            .count(),
        100
    );
}

#[test]
fn printed_faces_snapshots_and_inline_ability_payloads_are_conservative_candidates() {
    let mut engine = engine();
    let mut saved = definition("saved", "Enchantment");
    saved.rules = vec![source_rule()];
    engine.state.players[0].battlefield[0].printed_definition = Some(saved.clone());
    assert!(engine.quantity_known_sources_may_enable());
    engine.state.players[0].battlefield[0].printed_definition = None;
    assert!(!engine.quantity_known_sources_may_enable());
    engine
        .state
        .rule_modifiers
        .push(json!({"kind":"temporaryCopyUntilEndOfTurn","originalDefinition":saved}));
    assert!(engine.quantity_known_sources_may_enable());
    engine.state.rule_modifiers.clear();
    engine.state.game_pieces.push(saved);
    assert!(engine.quantity_known_sources_may_enable());
    engine.state.game_pieces.clear();
    for payload in [
        json!({"kind":"grantAbility","ability":source_rule()}),
        json!({"kind":"replaceAbility","to":source_rule()}),
        json!({"kind":"conjureNamedCard","name":"unknown external definition"}),
    ] {
        assert!(payload_risk(&payload, None, 0));
    }
    assert!(!payload_risk(&create_ordinary(1), None, 0));
}

#[test]
fn named_token_preflight_uses_typed_metadata_and_never_compiles_raw_text() {
    let mut source = definition("source", "Artifact");
    source.rules = vec![
        json!({"kind":"rulesMarker","text":"metadata","relatedTokenDefinitions":[definition("ordinary", "Artifact") ]}),
    ];
    let named = json!({"kind":"namedToken","name":"ordinary"});
    assert!(!payload_risk(&named, Some(&source), 0));
    source.rules[0]["relatedTokenDefinitions"][0]["rules"] = json!([source_rule()]);
    assert!(payload_risk(&named, Some(&source), 0));
    source.rules = vec![
        json!({"kind":"rulesMarker","text":"metadata","relatedTokenOracle":[{"name":"ordinary","oracleText":"unparsed on purpose"}]}),
    ];
    assert!(payload_risk(&named, Some(&source), 0));
    assert!(payload_risk(&named, None, 0));
    assert!(payload_risk(
        &json!({"kind":"namedToken","name":"not a known built-in"}),
        Some(&source),
        0
    ));
    assert!(!definition_may_enable_quantity(&definition(
        "plain", "Artifact"
    )));
}
