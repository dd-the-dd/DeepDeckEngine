use super::*;
use rand::RngCore;

fn definition(id: &str, kind: &str) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":kind,"power":"2","toughness":"2","rules":[]}),
    )
    .unwrap()
}
fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Sorcery"); 4],
                })
                .collect(),
        },
        7731,
    )
    .unwrap()
}
fn add(engine: &mut GameEngine, holder: usize, controller: &str, id: &str, index: usize) -> String {
    let mut card = card_instance(controller, &definition(id, "Land"), index);
    let id = card.instance_id.clone();
    card.controller = controller.to_owned();
    engine.state.players[holder].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction admission before execution or rollback snapshot");
    id
}
struct Selection {
    mode: usize,
    calls: usize,
}
impl DecisionProvider for Selection {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("expected structured sacrifice choice"))
    }
    fn choose_card_instance_ids(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        self.calls += 1;
        if self.mode == 4 {
            return Err(EngineError::new("ordinary-sacrifice-provider-sentinel"));
        }
        let Some(DecisionChoice::CardSelection {
            candidate_card_instance_ids: ids,
            ..
        }) = &request.choice
        else {
            panic!("card selection")
        };
        let last = ids.last().unwrap().clone();
        Ok(match self.mode {
            0 => vec![last],
            1 => vec![last.clone(), last],
            2 => vec!["not-a-candidate".to_owned()],
            _ => Vec::new(),
        })
    }
}
fn execute(
    engine: &mut GameEngine,
    provider: &mut Selection,
    count: usize,
) -> Result<(), EngineError> {
    engine.execute_canonical_rule(CanonicalRuleExecution {
        controller_id:"p0".into(), source_definition_id:"source".into(),
        rule:json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
            {"kind":"gainLife","player":{"kind":"abilityController"},"amount":2},
            {"kind":"sacrificePermanents","player":{"kind":"abilityController"},"where":{"kind":"cardTypeContains","value":"Land"},"count":count}
        ]}), decisions:BTreeMap::new(),targets:BTreeMap::new(),
    }, provider).map(|_| ())
}

#[test]
fn actual_controller_not_holder_and_phasing_define_eligible_domain() {
    let mut engine = engine();
    engine.state.players[0].battlefield.push(card_instance(
        "p0",
        &definition("source", "Artifact"),
        50,
    ));
    let id = add(&mut engine, 1, "p0", "controlled-in-another-bucket", 51);
    add(&mut engine, 0, "p0", "phased", 52);
    engine.state.players[0]
        .battlefield
        .last_mut()
        .unwrap()
        .flags
        .insert("phasedOut".into(), true);
    let mut provider = Selection { mode: 0, calls: 0 };
    execute(&mut engine, &mut provider, 1).unwrap();
    assert_eq!(
        provider.calls, 0,
        "one eligible object auto-selects without provider"
    );
    assert!(
        engine
            .state
            .players
            .iter()
            .flat_map(|p| &p.graveyard)
            .any(|c| c.instance_id == id)
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|c| c.definition.id == "phased")
    );
}

#[test]
fn full_structured_domain_and_invalid_callbacks_use_shared_checkpoint() {
    for mode in 0..5 {
        let mut engine = engine();
        engine.state.players[0].battlefield.push(card_instance(
            "p0",
            &definition("source", "Artifact"),
            50,
        ));
        for index in 0..130 {
            add(&mut engine, 0, "p0", &format!("land-{index}"), 100 + index);
        }
        let mut before = engine.clone();
        let mut provider = Selection { mode, calls: 0 };
        let result = execute(&mut engine, &mut provider, 1);
        assert_eq!(provider.calls, 1);
        if mode == 0 {
            result.unwrap();
            assert!(
                engine.state.players[0]
                    .graveyard
                    .iter()
                    .any(|c| c.definition.id == "land-129")
            );
        } else {
            result.unwrap_err();
            assert_eq!(
                serde_json::to_value(&engine.state).unwrap(),
                serde_json::to_value(&before.state).unwrap()
            );
            assert_eq!(engine.rng.next_u64(), before.rng.next_u64());
        }
    }
}
