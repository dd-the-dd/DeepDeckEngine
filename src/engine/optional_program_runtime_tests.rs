//! Standalone module under engine/mod.rs; no Oracle compilation dependency.
use super::*;

struct ProgramChoices {
    target_calls: usize,
    permission_calls: usize,
    accept: bool,
    fail_permission: bool,
}

impl DecisionProvider for ProgramChoices {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(
            request.player_id, "player-0",
            "the source controller chooses, never the opponent controlling the target"
        );
        if request.id.starts_with("triggerTargets:") {
            self.target_calls += 1;
            assert_eq!(request.options.len(), 2);
            Ok(0)
        } else {
            self.permission_calls += 1;
            if self.fail_permission {
                Err(EngineError::new("refused optional-program choice"))
            } else {
                Ok(usize::from(self.accept))
            }
        }
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"manaCost":"{0}","power":"2","toughness":"1","rules":[]})).unwrap()
}

fn setup() -> GameEngine {
    let filler = definition("filler", "Basic Land - Forest");
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
        },
        9817,
    )
    .unwrap();
    engine.state.step = GameStep::PrecombatMain;
    let rule: Value = serde_json::from_str(include_str!("optional_program_rule.json")).unwrap();
    assert!(rule_is_executable(&rule));
    let mut sage = definition("sage", "Creature - Elf Shaman");
    sage.rules = vec![rule];
    engine.state.players[0]
        .hand
        .push(card_instance("player-0", &sage, 1));
    engine.state.players[1].battlefield.push(card_instance(
        "player-1",
        &definition("target", "Artifact"),
        2,
    ));
    // A sole legal declaration is selected automatically. Two candidates
    // exercise the provider and prove who chooses before the permission.
    engine.state.players[1].battlefield.push(card_instance(
        "player-1",
        &definition("second-target", "Artifact"),
        3,
    ));
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine
}

#[test]
fn optional_program_declares_targets_before_permission_and_preserves_resolution_atomicity() {
    for scenario in ["accept", "decline", "illegal-target", "provider-failure"] {
        let mut engine = setup();
        let mut choices = ProgramChoices {
            target_calls: 0,
            permission_calls: 0,
            accept: scenario != "decline",
            fail_permission: false,
        };
        engine
            .cast_canonical_spell(
                &CanonicalSpellCast {
                    controller_id: "player-0".into(),
                    source_definition_id: "sage".into(),
                    use_alternative_cost: false,
                    alternative_exile_definition_id: None,
                    declaration_decisions: BTreeMap::new(),
                },
                &mut choices,
            )
            .unwrap();
        engine.resolve_canonical_top_stack(&mut choices).unwrap();
        // The low-level resolver leaves newly queued trigger declarations
        // pending; the priority boundary commits them before any response.
        engine
            .commit_pending_triggered_targets(&mut choices)
            .unwrap();
        assert_eq!(
            engine.state.stack.len(),
            1,
            "{scenario}: actual ETB trigger exists"
        );
        assert_eq!(
            choices.target_calls, 1,
            "{scenario}: target declared on trigger creation"
        );
        assert_eq!(
            choices.permission_calls, 0,
            "{scenario}: may choice is deferred to resolution"
        );
        if scenario == "illegal-target" {
            engine.state.players[1].battlefield.clear();
        }
        if scenario == "provider-failure" {
            let before = engine.state.clone();
            choices.fail_permission = true;
            let error = engine
                .resolve_canonical_top_stack(&mut choices)
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("refused optional-program choice")
            );
            assert_eq!(
                engine.state, before,
                "provider failure restores stack, targets, pending events and game state"
            );
            choices.fail_permission = false;
        }
        engine.resolve_canonical_top_stack(&mut choices).unwrap();
        assert!(
            engine.state.stack.is_empty(),
            "{scenario}: trigger resolves or fizzles"
        );
        let destroyed = engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "permanentDestroyed")
            .count();
        assert_eq!(
            destroyed,
            usize::from(matches!(scenario, "accept" | "provider-failure")),
            "{scenario}: optional action outcome"
        );
        if scenario == "illegal-target" {
            assert_eq!(
                choices.permission_calls, 0,
                "illegal required target skips the optional choice"
            );
        } else if scenario == "decline" {
            assert_eq!(
                engine.state.players[1].battlefield.len(),
                2,
                "decline preserves the target"
            );
        }
    }
}
