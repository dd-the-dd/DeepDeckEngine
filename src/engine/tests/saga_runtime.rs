use super::*;
use crate::engine::saga::{SagaChapterPolicy, saga_chapters_for_transition};

fn chapter_rule(chapters: &[i32]) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "sagaChapterReached",
            "object": { "kind": "self" },
            "chapters": chapters
                .iter()
                .map(|chapter| json!({ "kind": "integer", "value": chapter }))
                .collect::<Vec<_>>(),
        },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": { "kind": "integer", "value": 1 },
        }],
    })
}

fn saga_definition(id: &str) -> CardDefinition {
    let mut definition = test_definition(id, "Enchantment - Saga");
    definition.rules = vec![chapter_rule(&[1, 2, 3])];
    definition
}

fn exact_cost_artifact_search_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "sagaChapterReached",
            "object": { "kind": "self" },
            "chapters": [{ "kind": "integer", "value": 3 }],
        },
        "effects": [
            {
                "kind": "chooseCards",
                "id": "searchedCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "minimum": 0,
                "maximum": 1,
                "candidates": {
                    "kind": "cards",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                    "where": {
                        "kind": "and",
                        "operands": [
                            { "kind": "cardTypeContains", "value": "Artifact" },
                            {
                                "kind": "or",
                                "operands": [
                                    { "kind": "manaCostEquals", "value": "{0}" },
                                    { "kind": "manaCostEquals", "value": "{1}" },
                                ],
                            },
                        ],
                    },
                },
            },
            {
                "kind": "moveCards",
                "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                "to": {
                    "kind": "battlefield",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "tapped": false,
                },
            },
            {
                "kind": "shuffleZone",
                "zone": {
                    "kind": "library",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
            },
            {
                "kind": "recordLibrarySearch",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
            },
        ],
    })
}

fn construct_ability_grant_rule() -> Value {
    let counted_artifacts = json!({
        "kind": "countPermanents",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "where": { "kind": "cardTypeContains", "value": "Artifact" },
    });
    let characteristic_ability = json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "modifyPowerToughness",
            "objects": { "kind": "self" },
            "power": counted_artifacts.clone(),
            "toughness": counted_artifacts,
        }],
    });
    let construct_token = json!({
        "colors": [],
        "power": 0,
        "toughness": 0,
        "types": ["Artifact", "Creature"],
        "subtypes": ["Construct"],
        "abilities": [characteristic_ability],
    });
    let granted_ability = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [
            { "kind": "payMana", "manaCost": "{2}" },
            { "kind": "tap", "object": { "kind": "self" } },
        ],
        "effects": [{
            "kind": "createTokens",
            "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
            "quantity": { "kind": "integer", "value": 1 },
            "token": construct_token,
        }],
    });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "sagaChapterReached",
            "object": { "kind": "self" },
            "chapters": [{ "kind": "integer", "value": 2 }],
        },
        "effects": [{
            "kind": "grantAbility",
            "object": { "kind": "self" },
            "duration": { "kind": "permanent" },
            "ability": granted_ability,
        }],
    })
}

#[test]
fn lore_transitions_trigger_every_crossed_chapter_but_read_ahead_only_its_choice() {
    assert_eq!(
        saga_chapters_for_transition(0, 3, SagaChapterPolicy::CrossedThresholds),
        vec![1, 2, 3]
    );
    assert_eq!(
        saga_chapters_for_transition(0, 3, SagaChapterPolicy::ReadAheadSelectedOnly(2)),
        vec![2]
    );
    assert!(saga_chapters_for_transition(2, 2, SagaChapterPolicy::CrossedThresholds).is_empty());
}

#[test]
fn generic_lore_counter_placement_records_counters_and_narrows_grouped_chapters() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].battlefield.push(test_instance(
        "crossing-saga",
        saga_definition("crossing-saga"),
        "player-0",
    ));

    assert_eq!(
        engine.put_counters_on_permanent("crossing-saga", "lore", 2),
        2
    );
    assert_eq!(engine.state.stack.len(), 2);
    let triggered_chapters = engine
        .state
        .stack
        .iter()
        .map(|stack_object| {
            stack_object.ability_rule.as_ref().unwrap()["event"]["chapters"][0]["value"]
                .as_i64()
                .unwrap()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(triggered_chapters, BTreeSet::from([1, 2]));
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "countersPlaced"
            && event.card_instance_id.as_deref() == Some("crossing-saga")
            && event.detail["counter"] == "lore"
            && event.detail["count"] == 2
            && event.detail["oldCount"] == 0
            && event.detail["newCount"] == 2
    }));
}

#[test]
fn saga_entry_lore_and_chapter_are_not_suppressed_with_etb_triggers() {
    let mut suppressor_definition = test_definition("etb-suppressor", "Artifact");
    suppressor_definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "suppressTriggerCreation",
            "causedBy": {
                "kind": "enterBattlefield",
                "object": {
                    "kind": "eventObject",
                    "where": { "kind": "cardTypeContains", "value": "Enchantment" },
                },
            }
        }]
    })];
    let mut saga = test_instance(
        "entering-saga",
        saga_definition("entering-saga"),
        "player-0",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].battlefield.push(test_instance(
        "etb-suppressor",
        suppressor_definition,
        "player-0",
    ));
    engine.mark_saga_entry_lore_pending(&mut saga, None);
    // The initial lore counter is placed by the entry replacement pipeline
    // (CR 714.3a) before the Saga is put onto the battlefield.
    engine
        .apply_enter_replacements(&mut saga, &mut EmeritusDecisionProvider)
        .expect("entry replacements apply");
    engine.state.players[0].battlefield.push(saga.clone());

    engine.enqueue_enter_battlefield_triggers(&saga);

    let entered = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "entering-saga")
        .unwrap();
    assert_eq!(entered.counters.get("lore"), Some(&1));
    assert_eq!(engine.state.stack.len(), 1);
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "enterBattlefieldTriggersSuppressed"
            && event.card_instance_id.as_deref() == Some("entering-saga")
    }));
}

#[test]
fn second_chapter_grants_and_executes_the_reusable_construct_activation() {
    let mut definition = test_definition("urzas-saga", "Enchantment Land - Urza's Saga");
    definition.name = "Urza's Saga".to_string();
    definition.rules = vec![construct_ability_grant_rule()];
    let saga = test_instance("urzas-saga", definition, "player-0");
    let support_artifact = test_instance(
        "support-artifact",
        test_definition("support-artifact", "Artifact"),
        "player-0",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].battlefield = vec![saga.clone(), support_artifact];
    engine.enqueue_saga_chapter_triggers(&saga, 2);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("chapter II grants the activated ability");
    for _ in 0..2 {
        engine.state.players[0].mana_pool.push(FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        });
    }
    engine
        .activate_canonical_ability(
            &CanonicalActivatedAbilityActivation {
                controller_id: "player-0".to_string(),
                source_definition_id: "urzas-saga".to_string(),
                ability_rule_index: Some(1),
                targets: BTreeMap::new(),
            },
            &mut EmeritusDecisionProvider,
        )
        .expect("the granted ability pays two mana and taps the Saga");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the granted ability creates a Construct");

    let construct = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| {
            permanent.definition.is_token
                && type_line_contains(&permanent.definition.type_line, "Construct")
        })
        .expect("a Construct token exists");
    assert_eq!(engine.current_power(construct), 2);
    assert_eq!(engine.current_toughness(construct), 2);
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|permanent| permanent.instance_id == "urzas-saga")
            .unwrap()
            .tapped
    );
}

#[test]
fn completed_saga_waits_only_for_its_final_chapter_ability() {
    let definition = saga_definition("completed-saga");
    let mut saga = test_instance("completed-saga", definition, "player-0");
    saga.counters.insert("lore".to_string(), 3);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![saga.clone()];

    let mut nonfinal_rule = chapter_rule(&[2]);
    nonfinal_rule["event"]["chapters"] = json!([{
        "kind": "integer",
        "value": 2,
    }]);
    engine.state.stack.push(StackObject {
        id: "stack:completed-saga:chapter-two".to_string(),
        controller: "player-0".to_string(),
        card: saga.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(nonfinal_rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });

    assert_eq!(
        engine.completed_sagas_ready_to_sacrifice(),
        vec!["completed-saga".to_string()]
    );

    engine.state.stack.push(StackObject {
        id: "stack:completed-saga:chapter-three".to_string(),
        controller: "player-0".to_string(),
        card: saga,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(chapter_rule(&[3])),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    assert!(engine.completed_sagas_ready_to_sacrifice().is_empty());
}

#[test]
fn final_chapter_with_no_search_candidate_still_shuffles_and_sacrifices_the_saga() {
    let final_rule = exact_cost_artifact_search_rule();
    let mut definition = test_definition("urzas-saga", "Enchantment Land - Urza's Saga");
    definition.name = "Urza's Saga".to_string();
    definition.rules = vec![final_rule];
    let mut saga = test_instance("urzas-saga", definition, "player-0");
    saga.counters.insert("lore".to_string(), 3);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![saga.clone()];
    engine.state.players[0].library = vec![test_instance(
        "ineligible-card",
        test_definition("ineligible-card", "Instant"),
        "player-0",
    )];
    engine.enqueue_saga_chapter_triggers(&saga, 3);

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the empty search resolves and the final chapter leaves the stack");

    assert!(engine.permanent_position("urzas-saga").is_none());
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "urzas-saga")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "librarySearched" && event.card_instance_id.as_deref() == Some("urzas-saga")
    }));
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "zoneShuffled" && event.card_instance_id.as_deref() == Some("urzas-saga")
    }));
}

#[test]
fn countered_final_chapter_still_allows_the_completed_saga_to_be_sacrificed() {
    let mut definition = test_definition("countered-saga", "Enchantment - Saga");
    definition.rules = vec![chapter_rule(&[3])];
    let mut saga = test_instance("countered-saga", definition, "player-0");
    saga.counters.insert("lore".to_string(), 3);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![saga.clone()];
    engine.enqueue_saga_chapter_triggers(&saga, 3);
    engine.state.stack.clear();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    engine.check_state_based_actions();

    assert!(engine.permanent_position("countered-saga").is_none());
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "permanentToGraveyard"
            && event.card_instance_id.as_deref() == Some("countered-saga")
            && event.detail["reason"] == "sacrificed"
    }));
}
