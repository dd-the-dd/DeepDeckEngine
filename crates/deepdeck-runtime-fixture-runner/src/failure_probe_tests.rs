use super::*;
use serde_json::json;

fn fixture(actions: Value) -> RuntimeFixture {
    serde_json::from_value(json!({"schemaVersion":FIXTURE_SCHEMA_VERSION,"id":"generic-failure-probe-test","rule":{"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}]},"initialState":{"activePlayerId":"p0","players":[{"id":"p0","name":"P0","battlefield":[{"id":"executor","name":"Executor","typeLine":"Enchantment","rules":[]}]},{"id":"p1","name":"P1"}]},"actions":actions,"assertions":[{"kind":"lifeTotal","playerId":"p0","equals":22}]})).unwrap()
}
fn execute() -> Value {
    json!({"kind":"executeRule","execution":{"controllerId":"p0","sourceDefinitionId":"executor"}})
}

#[test]
fn legacy_fixture_fields_and_result_assertion_count_remain_unchanged() {
    let mut value=serde_json::to_value(json!({"schemaVersion":FIXTURE_SCHEMA_VERSION,"id":"legacy","rule":{"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}]},"initialState":{"activePlayerId":"p0","players":[{"id":"p0","name":"P0","battlefield":[{"id":"executor","name":"Executor","typeLine":"Enchantment","rules":[]}]},{"id":"p1","name":"P1"}]},"execution":{"controllerId":"p0","sourceDefinitionId":"executor"},"assertions":[{"kind":"lifeTotal","playerId":"p0","equals":22}]})).unwrap();
    let parsed: RuntimeFixture = serde_json::from_value(value.take()).unwrap();
    let result = run_fixture(parsed);
    assert!(matches!(result.status, FixtureStatus::Passed));
    assert_eq!(result.assertions.len(), 1);
}
#[test]
fn schema_rejects_unknown_provider_fields_and_missing_error_expectation() {
    assert!(
        serde_json::from_value::<ProviderErrorInjection>(
            json!({"atDecision":1,"message":"fault","unknown":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<FixtureAction>(
            json!({"kind":"probeFailure","action":{"kind":"resolveTopStack"}})
        )
        .is_err()
    );
}
#[test]
fn semantic_validation_rejects_zero_index_nested_probe_and_nonrecoverable_auto_retry() {
    for action in [
        json!({"kind":"probeFailure","action":{"kind":"resolveTopStack"},"expectedErrorContains":"fault","providerError":{"atDecision":0,"message":"fault"}}),
        json!({"kind":"probeFailure","action":{"kind":"probeFailure","action":{"kind":"resolveTopStack"},"expectedErrorContains":"fault"},"expectedErrorContains":"fault"}),
        json!({"kind":"probeFailure","action":{"kind":"resolveTopStack"},"expectedErrorContains":"fault","retry":true}),
    ] {
        let action: FixtureAction = serde_json::from_value(action).unwrap();
        assert!(validate(&action).is_err());
    }
}
#[test]
fn an_expected_error_cannot_pass_when_the_action_succeeds_or_never_reaches_injection() {
    for injection in [json!(null), json!({"atDecision":1,"message":"fault"})] {
        let result = run_fixture(fixture(
            json!([{"kind":"probeFailure","action":execute(),"expectedErrorContains":"fault","providerError":injection}]),
        ));
        assert!(matches!(result.status, FixtureStatus::Failed));
        assert!(result.error.unwrap().contains("unexpectedly succeeded"));
    }
}
#[test]
fn every_fallible_provider_method_can_fail_before_consuming_a_scripted_answer() {
    let engine = probe_position();
    let request: EngineDecisionRequest = serde_json::from_value(
        json!({"id":"probe","kind":"resolutionChoice","playerId":"p0","options":[]}),
    )
    .unwrap();
    for method in [
        ProviderMethod::ChooseOption,
        ProviderMethod::ChooseNumber,
        ProviderMethod::ChooseCardName,
        ProviderMethod::ChooseCardInstanceIds,
        ProviderMethod::ChooseLoopShortcutPolicy,
    ] {
        let sentinel = ScriptedDecision::ChooseOption { index: 0 };
        let expected_queue = VecDeque::from([sentinel.clone()]);
        let mut inner = ScriptedProvider {
            decisions: expected_queue.clone(),
            calls: 0,
        };
        let injection = ProviderErrorInjection {
            at_decision: 1,
            message: "fault".into(),
            method: Some(method),
            player_id: Some("p0".into()),
            request_id: Some("probe".into()),
        };
        let mut wrapper = ProbeProvider {
            inner: &mut inner,
            failure: Some(&injection),
            visits: vec![],
            injected: false,
        };
        let result = match method {
            ProviderMethod::ChooseOption => wrapper.choose(engine.state(), &request).map(|_| ()),
            ProviderMethod::ChooseNumber => {
                wrapper.choose_number(engine.state(), &request).map(|_| ())
            }
            ProviderMethod::ChooseCardName => wrapper
                .choose_card_name(engine.state(), &request)
                .map(|_| ()),
            ProviderMethod::ChooseCardInstanceIds => wrapper
                .choose_card_instance_ids(engine.state(), &request)
                .map(|_| ()),
            ProviderMethod::ChooseLoopShortcutPolicy => wrapper
                .choose_loop_shortcut_policy(engine.state(), &request)
                .map(|_| ()),
        };
        assert_eq!(result.unwrap_err().to_string(), "fault");
        assert!(wrapper.injected);
        drop(wrapper);
        assert_eq!(inner.calls, 1);
        assert_eq!(
            inner.decisions, expected_queue,
            "fault must not consume the nonempty sentinel queue"
        );
    }
}

fn probe_position() -> GameEngine {
    let base = fixture(json!([execute()]));
    let mut bootstrap = ScriptedProvider {
        decisions: VecDeque::new(),
        calls: 0,
    };
    build_engine(
        &base.initial_state,
        1,
        &base.rule,
        &BTreeSet::new(),
        &mut bootstrap,
    )
    .unwrap()
}

fn request() -> EngineDecisionRequest {
    serde_json::from_value(json!({"id":"probe","kind":"resolutionChoice","playerId":"p0","options":[{"id":"option-0","kind":"passPriority","playerId":"p0","label":"First"},{"id":"option-1","kind":"passPriority","playerId":"p0","label":"Second"}]})).unwrap()
}
fn permission_action() -> FixtureAction {
    let mut action = execute();
    action["rule"] = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"optionalEffects","player":{"kind":"abilityController"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}]}]});
    serde_json::from_value(action).unwrap()
}
#[test]
fn guard_mismatch_preserves_nonempty_queue_and_cannot_count_as_injection_even_with_matching_text() {
    let message = "DATA provider injection request did not match its expected method/player/id";
    let engine = probe_position();
    let request = request();
    for guard in ["method", "player", "request"] {
        let mut injection = ProviderErrorInjection {
            at_decision: 1,
            message: message.into(),
            method: None,
            player_id: None,
            request_id: None,
        };
        match guard {
            "method" => injection.method = Some(ProviderMethod::ChooseNumber),
            "player" => injection.player_id = Some("wrong-player".into()),
            _ => injection.request_id = Some("wrong-request".into()),
        }
        let expected = VecDeque::from([ScriptedDecision::ChooseOption { index: 1 }]);
        let mut inner = ScriptedProvider {
            decisions: expected.clone(),
            calls: 0,
        };
        let mut wrapper = ProbeProvider {
            inner: &mut inner,
            failure: Some(&injection),
            visits: vec![],
            injected: false,
        };
        assert_eq!(
            wrapper
                .choose(engine.state(), &request)
                .unwrap_err()
                .to_string(),
            message
        );
        assert!(!wrapper.injected);
        assert_eq!(wrapper.visits.len(), 1);
        drop(wrapper);
        assert_eq!(inner.calls, 0);
        assert_eq!(inner.decisions, expected);
        // Exercise the complete probe gate with identical expected/error text.
        // No final fixture assertion or leftover-queue gate can mask a false pass.
        let mut actual = probe_position();
        let before = actual.state().clone();
        let mut provider = ScriptedProvider {
            decisions: VecDeque::new(),
            calls: 0,
        };
        let error = run(
            &mut actual,
            &permission_action(),
            &json!({}),
            &mut provider,
            message,
            Some(&injection),
            false,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("did not reach the configured provider injection")
        );
        assert_eq!(actual.state(), &before);
        assert_eq!(provider.calls, 0);
    }
}
#[test]
fn natural_provider_error_before_fault_index_cannot_pass_using_the_identical_message() {
    let message = "script exhausted at decision 1 (chooseOption)";
    let mut actual = probe_position();
    let before = actual.state().clone();
    let injection = ProviderErrorInjection {
        at_decision: 2,
        message: message.into(),
        method: Some(ProviderMethod::ChooseOption),
        player_id: None,
        request_id: None,
    };
    let mut provider = ScriptedProvider {
        decisions: VecDeque::new(),
        calls: 0,
    };
    {
        let mut wrapper = ProbeProvider {
            inner: &mut provider,
            failure: Some(&injection),
            visits: vec![],
            injected: false,
        };
        assert_eq!(
            wrapper
                .choose(actual.state(), &request())
                .unwrap_err()
                .to_string(),
            message
        );
        assert!(!wrapper.injected);
        assert_eq!(wrapper.visits.len(), 1);
    }
    assert_eq!(provider.calls, 1);
    provider.calls = 0;
    let error = run(
        &mut actual,
        &permission_action(),
        &json!({}),
        &mut provider,
        message,
        Some(&injection),
        false,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("did not reach the configured provider injection")
    );
    assert_eq!(
        provider.calls, 1,
        "one real callback failed naturally before the configured second callback"
    );
    assert_eq!(actual.state(), &before);
}
#[test]
fn five_successful_callback_delegations_preserve_values_counts_and_default_loop_answer() {
    let engine = probe_position();
    let cases=[
        (ProviderMethod::ChooseOption,ScriptedDecision::ChooseOption{index:1},request(),json!(1)),
        (ProviderMethod::ChooseNumber,ScriptedDecision::ChooseNumber{value:7},request(),json!(7)),
        (ProviderMethod::ChooseCardName,ScriptedDecision::ChooseCardName{value:"Example Card AbC".into()},request(),json!("Example Card AbC")),
        (ProviderMethod::ChooseCardInstanceIds,ScriptedDecision::SelectCards{count:1,take:SelectionOrder::Reverse},serde_json::from_value(json!({"id":"probe","kind":"resolutionChoice","playerId":"p0","choice":{"kind":"cardSelection","decisionId":"cards","candidateCardInstanceIds":["a","b"],"minimum":1,"maximum":1,"prompt":"Select"},"options":[]})).unwrap(),json!(["b"])),
    ];
    for (method, answer, request, expected) in cases {
        let mut inner = ScriptedProvider {
            decisions: VecDeque::from([answer]),
            calls: 0,
        };
        let mut wrapper = ProbeProvider {
            inner: &mut inner,
            failure: None,
            visits: vec![],
            injected: false,
        };
        let result = match method {
            ProviderMethod::ChooseOption => {
                wrapper.choose(engine.state(), &request).map(|v| json!(v))
            }
            ProviderMethod::ChooseNumber => wrapper
                .choose_number(engine.state(), &request)
                .map(|v| json!(v)),
            ProviderMethod::ChooseCardName => wrapper
                .choose_card_name(engine.state(), &request)
                .map(|v| json!(v)),
            ProviderMethod::ChooseCardInstanceIds => wrapper
                .choose_card_instance_ids(engine.state(), &request)
                .map(|v| json!(v)),
            ProviderMethod::ChooseLoopShortcutPolicy => unreachable!(),
        };
        assert_eq!(result.unwrap(), expected);
        assert!(!wrapper.injected);
        assert_eq!(wrapper.visits, vec![Visit { method, request }]);
        drop(wrapper);
        assert_eq!(inner.calls, 1);
        assert!(inner.decisions.is_empty());
    }
    for queue in [
        VecDeque::new(),
        VecDeque::from([ScriptedDecision::ChooseOption { index: 1 }]),
    ] {
        let mut inner = ScriptedProvider {
            decisions: queue.clone(),
            calls: 0,
        };
        let mut wrapper = ProbeProvider {
            inner: &mut inner,
            failure: None,
            visits: vec![],
            injected: false,
        };
        assert_eq!(
            wrapper
                .choose_loop_shortcut_policy(engine.state(), &request())
                .unwrap(),
            0
        );
        assert_eq!(wrapper.visits.len(), 1);
        assert!(!wrapper.injected);
        drop(wrapper);
        assert_eq!(inner.calls, 1);
        assert_eq!(
            inner.decisions, queue,
            "default loop policy must neither require nor consume a scripted answer"
        );
    }
}
