use super::super::*;
use super::counter_collection_selector_supported;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("unexpected counter collection prompt"))
    }
}
fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":types,"manaCost":"{1}","power":"2","toughness":"2"}),
    )
    .unwrap()
}
fn world() -> (GameEngine, StackObject, BTreeMap<String, String>) {
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
        704,
    )
    .unwrap();
    let mut animator = card_instance("player-0", &definition("animator", "Enchantment"), 1);
    animator.definition.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"addCardType","objects":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Land"}},"cardType":"Creature"}]}),
    ];
    let frame = StackObject {
        id: "collection".to_string(),
        controller: "player-0".to_string(),
        card: animator.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let land = card_instance("player-0", &definition("land", "Land"), 2);
    let own = card_instance("player-0", &definition("own", "Creature"), 3);
    let mut former = card_instance("player-0", &definition("former", "Artifact Creature"), 4);
    former.definition.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"removeCardTypeWhile","object":{"kind":"self"},"cardType":"Creature","condition":{"kind":"controlsPermanent","where":{"kind":"cardTypeContains","value":"Land"}}}]}),
    ];
    let enemy = card_instance("player-1", &definition("enemy", "Creature"), 5);
    let ids = BTreeMap::from([
        ("land".to_string(), land.instance_id.clone()),
        ("own".to_string(), own.instance_id.clone()),
        ("former".to_string(), former.instance_id.clone()),
        ("enemy".to_string(), enemy.instance_id.clone()),
    ]);
    engine.state.players[0].battlefield = vec![animator, land, own, former];
    engine.state.players[1].battlefield = vec![enemy];
    (engine, frame, ids)
}
fn selector(scope: Option<Value>) -> Value {
    let mut s =
        json!({"kind":"eachPermanent","where":{"kind":"cardTypeContains","value":"Creature"}});
    if let Some(scope) = scope {
        s["player"] = scope;
    }
    s
}
#[test]
fn collections_use_effective_types_in_both_directions() {
    let (engine, frame, ids) = world();
    let selected = engine
        .counter_collection_ids(
            &selector(Some(json!({"kind":"abilityController"}))),
            &frame,
            &BTreeMap::new(),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(
        selected.into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([ids["land"].clone(), ids["own"].clone()])
    );
    let (p, c) = engine.permanent_position(&ids["land"]).unwrap();
    assert!(!has_card_type(
        &engine.state.players[p].battlefield[c].definition,
        "Creature"
    ));
    assert!(engine.permanent_has_card_type(&engine.state.players[p].battlefield[c], "Creature"));
    let (p, c) = engine.permanent_position(&ids["former"]).unwrap();
    assert!(has_card_type(
        &engine.state.players[p].battlefield[c].definition,
        "Creature"
    ));
    assert!(!engine.permanent_has_card_type(&engine.state.players[p].battlefield[c], "Creature"));
}
#[test]
fn own_opponent_and_all_scopes_are_distinct_and_use_common_player_resolution() {
    let (engine, frame, ids) = world();
    let own = engine
        .counter_collection_ids(
            &selector(Some(json!({"kind":"abilityController"}))),
            &frame,
            &BTreeMap::new(),
            &BTreeSet::new(),
        )
        .unwrap();
    let enemy = engine
        .counter_collection_ids(
            &selector(Some(
                json!({"kind":"opponentsOf","player":{"kind":"boundValue","id":"anchor"}}),
            )),
            &frame,
            &BTreeMap::from([(
                "anchor".to_string(),
                RuntimeBinding::Player("player-0".to_string()),
            )]),
            &BTreeSet::new(),
        )
        .unwrap();
    let each = engine
        .counter_collection_ids(
            &selector(Some(json!({"kind":"eachPlayer"}))),
            &frame,
            &BTreeMap::new(),
            &BTreeSet::new(),
        )
        .unwrap();
    let all = engine
        .counter_collection_ids(&selector(None), &frame, &BTreeMap::new(), &BTreeSet::new())
        .unwrap();
    assert_eq!(own.len(), 2);
    assert_eq!(enemy, vec![ids["enemy"].clone()]);
    assert_eq!(each.len(), 3);
    assert_eq!(all, each);
}
#[test]
fn explicit_missing_scope_is_an_error_before_any_counter_mutation() {
    let (mut engine, _, _) = world();
    let before = serde_json::to_value(&engine.state).unwrap();
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"putCounters","permanent":selector(Some(json!({"kind":"boundValue","id":"missing"}))),"counter":"charge","count":{"kind":"integer","value":1}}]});
    assert!(rule_is_executable(&rule));
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "player-0".to_string(),
                source_definition_id: "animator".to_string(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut NoChoices,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("explicit counter collection scope is unresolved")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}
#[test]
fn empty_collection_completes_without_any_prompt_or_counter_event() {
    let (mut engine, _, _) = world();
    engine.state.players[1].battlefield.clear();
    let before = engine.state.events.len();
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"putCounters","permanent":selector(Some(json!({"kind":"opponentsOf","player":{"kind":"abilityController"}}))),"counter":"charge","count":{"kind":"integer","value":1}}]});
    engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "player-0".to_string(),
                source_definition_id: "animator".to_string(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut NoChoices,
        )
        .unwrap();
    assert!(
        engine.state.events[before..]
            .iter()
            .all(|e| e.kind != "countersPlaced")
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|p| !p.counters.contains_key("charge"))
    );
}
#[test]
fn unrepresentable_player_sets_are_rejected_by_the_collection_contract() {
    for scope in [
        json!(null),
        json!({"kind":"randomPlayer"}),
        json!({"kind":"playersExcept"}),
        json!({"kind":"opponentsOf","player":{"kind":"eachPlayer"}}),
    ] {
        let operand = selector(Some(scope));
        assert!(!counter_collection_selector_supported(&operand));
        assert!(!effect_supported(
            &json!({"kind":"putCounters","permanent":operand,"counter":"charge","count":{"kind":"integer","value":1}})
        ));
    }
}
