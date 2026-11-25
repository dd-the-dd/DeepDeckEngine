use super::super::resolution_preflight::{requires_checkpoint, stack_requires_checkpoint};
use serde_json::json;

#[test]
fn nested_optional_program_and_action_local_permission_require_checkpoint() {
    assert!(requires_checkpoint(
        &json!({"effects":[{"kind":"conditionalEffects","then":[{"kind":"optionalEffects","effects":[]}]}]})
    ));
    assert!(requires_checkpoint(
        &json!({"effects":[{"kind":"drawCards","optional":true}]})
    ));
}

#[test]
fn ordinary_ir_and_permission_words_in_names_do_not_require_checkpoint() {
    assert!(!requires_checkpoint(
        &json!({"kind":"spellAbility","name":"optionalEffects","effects":[{"kind":"gainLife","optional":false,"amount":{"kind":"integer","value":2}}]})
    ));
}

#[test]
fn excessive_structural_depth_is_conservative() {
    let mut value = json!({"kind":"gainLife"});
    for _ in 0..65 {
        value = json!([value]);
    }
    assert!(requires_checkpoint(&value));
}

use super::super::*;
use rand::RngCore;

struct RollbackPermissionChoice {
    refuse: bool,
}

impl DecisionProvider for RollbackPermissionChoice {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        if self.refuse {
            Err(EngineError::new("refused rewritten-top permission"))
        } else {
            Ok(1)
        }
    }
}

fn rollback_stack_position() -> GameEngine {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"source","name":"source","typeLine":"Creature","manaCost":"{0}",
        "power":"2","toughness":"2","rules":[]
    }))
    .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 12],
                })
                .collect(),
        },
        10901,
    )
    .unwrap();
    let mut source = card_instance("player-0", &definition, 201);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source.clone());
    let optional = json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
        "effects":[{"kind":"optionalEffects","player":{"kind":"abilityController"},
            "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},
                "amount":{"kind":"integer","value":2}}]}]});
    let mandatory = json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
        "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets",
            "id":"missingArtifact","candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Artifact"}},"minimum":1,"maximum":1}]},
        "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]});
    for (id, rule) in [
        ("optional-underneath", optional),
        ("mandatory-target-top", mandatory),
    ] {
        engine.state.stack.push(StackObject {
            id: id.into(),
            controller: "player-0".into(),
            card: source.clone(),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("triggeredAbility".into()),
            ability_rule: Some(rule),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        });
    }
    engine
}

#[test]
fn stack_permission_preflight_covers_rewritten_top_and_pending_declarations() {
    let mut engine = rollback_stack_position();
    assert!(stack_requires_checkpoint(&engine.state.stack));
    engine.state.stack.remove(0);
    assert!(
        stack_requires_checkpoint(&engine.state.stack),
        "pending target commitment may itself enqueue an optional targeting trigger"
    );
    engine.state.stack[0]
        .ability_rule
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("declaration");
    assert!(
        !stack_requires_checkpoint(&engine.state.stack),
        "deterministic no-declaration stack retains the zero-checkpoint path"
    );
}

#[test]
fn no_legal_top_targets_then_underlying_permission_error_restores_engine_and_retries() {
    let mut engine = rollback_stack_position();
    let before = engine.clone();
    let mut provider = RollbackPermissionChoice { refuse: true };
    let error = engine.resolve_top_stack(&mut provider).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refused rewritten-top permission")
    );
    assert_eq!(
        engine.state, before.state,
        "restore removed trigger, stack, events, targets, decisions and counters"
    );
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
    assert_eq!(engine.pending_empty_draws, before.pending_empty_draws);
    assert_eq!(engine.commander_last_zones, before.commander_last_zones);
    assert_eq!(engine.stack_sequence, before.stack_sequence);
    assert_eq!(engine.copy_sequence, before.copy_sequence);
    assert_eq!(engine.starting_player, before.starting_player);
    assert_eq!(engine.opening_hand_size, before.opening_hand_size);
    assert_eq!(engine.force_end_turn, before.force_end_turn);
    assert_eq!(
        engine.branching_decision_sequence.get(),
        before.branching_decision_sequence.get()
    );
    let mut rng_after = engine.rng.clone();
    let mut rng_before = before.rng.clone();
    assert_eq!(rng_after.next_u64(), rng_before.next_u64());
    assert!(!engine.quantity_transaction_in_progress());
    assert!(!engine.quantity_preflight_in_progress());
    assert_eq!(
        engine.quantity_snapshot_count(),
        before.quantity_snapshot_count() + 1
    );
    provider.refuse = false;
    engine.resolve_top_stack(&mut provider).unwrap();
    assert_eq!(engine.state.players[0].life, 22);
    assert!(engine.state.stack.is_empty());
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "abilityFizzled")
            .count(),
        1
    );
    assert_eq!(
        engine.quantity_snapshot_count(),
        before.quantity_snapshot_count() + 2
    );
}

#[test]
fn direct_optional_program_restores_state_on_refusal_and_can_retry() {
    let mut engine = rollback_stack_position();
    engine.state.stack.clear();
    // The direct API requires an unambiguous source across all zones.
    engine.state.players[0].battlefield[0].definition.id = "direct-executor".into();
    let execution = CanonicalRuleExecution {
        controller_id: "player-0".into(),
        source_definition_id: "direct-executor".into(),
        rule: json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
            {"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}},
            {"kind":"optionalEffects","player":{"kind":"abilityController"},"effects":[
                {"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}
            ]}
        ]}),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let before = engine.clone();
    let mut provider = RollbackPermissionChoice { refuse: true };
    let error = engine
        .execute_canonical_rule(execution.clone(), &mut provider)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refused rewritten-top permission")
    );
    assert_eq!(
        engine.state, before.state,
        "earlier mandatory action also rolls back"
    );
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
    assert_eq!(
        engine.quantity_snapshot_count(),
        before.quantity_snapshot_count() + 1
    );
    provider.refuse = false;
    engine
        .execute_canonical_rule(execution, &mut provider)
        .unwrap();
    assert_eq!(engine.state.players[0].life, 23);
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
}

#[path = "resolution_discard_preflight_tests.rs"]
mod discard_preflight;

#[test]
fn ordinary_card_selection_and_random_card_selection_request_shared_rollback() {
    for effect in [
        json!({"kind":"chooseCards","id":"search","minimum":0,"maximum":2}),
        json!({"kind":"chooseCards","selection":{"kind":"random"},"count":1}),
    ] {
        assert!(requires_checkpoint(&effect));
        assert!(requires_checkpoint(
            &json!({"kind":"spellAbility","effects":[effect]})
        ));
    }
    assert!(!requires_checkpoint(
        &json!({"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1},"name":"chooseCards"})
    ));
}

struct NoChoiceAsked;

impl DecisionProvider for NoChoiceAsked {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "no choice is asked: {}",
            request.id
        )))
    }
}

/// `For as long as that card remains exiled, its owner may play it.` (CR
/// 108.3, 611.2a): the permission belongs to the owner of the exiled card,
/// not to the ability's controller, and lasts across turns while the card
/// stays in exile.
#[test]
fn derived_owner_play_permission_follows_the_exiled_cards_owner_across_turns() {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"exiled","name":"exiled","typeLine":"Creature","manaCost":"{2}",
        "power":"2","toughness":"2","rules":[]
    }))
    .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 10],
                })
                .collect(),
        },
        611,
    )
    .unwrap();
    let card = card_instance("player-1", &definition, 40);
    let card_id = card.instance_id.clone();
    engine.state.players[1].exile.push(card);
    let stack_object = StackObject {
        id: "stack:release".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &definition, 41),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let card_reference = json!({"kind":"boundObject","binding":"exiledObject"});
    let effect = json!({
        "kind":"grantPermission",
        "player":{"kind":"ownerOf","object":card_reference},
        "action":{"kind":"play","card":card_reference,
            "normalTimingApplies":true,"normalCostsApply":true},
        "duration":{"kind":"whileInExile"},
    });
    assert!(effect_supported(&effect));
    let mut bindings = BTreeMap::from([(
        "exiledObject".to_string(),
        RuntimeBinding::Objects(vec![card_id.clone()]),
    )]);
    let mut decisions = BTreeMap::new();
    engine
        .execute_effect_in_batch(
            &effect,
            None,
            &stack_object,
            &mut bindings,
            &mut decisions,
            &mut NoChoiceAsked,
        )
        .unwrap();
    let permission = engine
        .state
        .permissions
        .iter()
        .find(|permission| permission["cardInstanceId"].as_str() == Some(card_id.as_str()))
        .cloned()
        .expect("the owner's permission is granted");
    assert_eq!(permission["playerId"].as_str(), Some("player-1"));
    assert!(permission.get("expiresAfterTurn").is_none());
    engine.state.turn_number += 3;
    assert!(engine.card_permission_in_scope(&permission, "player-1", None));
    assert!(!engine.card_permission_in_scope(&permission, "player-0", None));
}
