//! DATA-driven public-API failure/rollback/retry probes, not engine rollback.
use super::*;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderMethod {
    ChooseOption,
    ChooseNumber,
    ChooseCardName,
    ChooseCardInstanceIds,
    ChooseLoopShortcutPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderErrorInjection {
    pub at_decision: usize,
    pub message: String,
    #[serde(default)]
    pub method: Option<ProviderMethod>,
    #[serde(default)]
    pub player_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Visit {
    method: ProviderMethod,
    request: EngineDecisionRequest,
}
struct ProbeProvider<'a> {
    inner: &'a mut ScriptedProvider,
    failure: Option<&'a ProviderErrorInjection>,
    visits: Vec<Visit>,
    injected: bool,
}
impl ProbeProvider<'_> {
    fn before(
        &mut self,
        method: ProviderMethod,
        request: &EngineDecisionRequest,
    ) -> Result<(), EngineError> {
        self.visits.push(Visit {
            method,
            request: request.clone(),
        });
        if let Some(failure) = self.failure
            && failure.at_decision == self.visits.len()
        {
            if failure.method.is_some_and(|expected| expected != method)
                || failure
                    .player_id
                    .as_ref()
                    .is_some_and(|expected| expected != &request.player_id)
                || failure
                    .request_id
                    .as_ref()
                    .is_some_and(|expected| expected != &request.id)
            {
                return Err(EngineError::new(
                    "DATA provider injection request did not match its expected method/player/id",
                ));
            }
            self.injected = true;
            self.inner.calls += 1;
            return Err(EngineError::new(failure.message.clone()));
        }
        Ok(())
    }
}
impl DecisionProvider for ProbeProvider<'_> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.before(ProviderMethod::ChooseOption, request)?;
        self.inner.choose(state, request)
    }
    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        self.before(ProviderMethod::ChooseNumber, request)?;
        self.inner.choose_number(state, request)
    }
    fn choose_card_name(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        self.before(ProviderMethod::ChooseCardName, request)?;
        self.inner.choose_card_name(state, request)
    }
    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        self.before(ProviderMethod::ChooseCardInstanceIds, request)?;
        self.inner.choose_card_instance_ids(state, request)
    }
    fn choose_loop_shortcut_policy(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.before(ProviderMethod::ChooseLoopShortcutPolicy, request)?;
        self.inner.calls += 1;
        self.inner.choose_loop_shortcut_policy(state, request)
    }
    fn requests_explicit_priority_pass(&self, p: &str) -> bool {
        self.inner.requests_explicit_priority_pass(p)
    }
    fn allows_combat_declaration_revisions(&self, p: &str) -> bool {
        self.inner.allows_combat_declaration_revisions(p)
    }
    fn observe_turn_completed(&mut self, s: &GameState) {
        self.inner.observe_turn_completed(s)
    }
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    fn begin_loop_shortcut(&mut self, p: &str, a: &str, n: usize) {
        self.inner.begin_loop_shortcut(p, a, n)
    }
    fn begin_loop_iteration(&mut self, p: &str, a: &str) {
        self.inner.begin_loop_iteration(p, a)
    }
}

pub(super) fn validate(action: &FixtureAction) -> Result<&FixtureAction, String> {
    if let FixtureAction::ProbeFailure {
        action,
        expected_error_contains,
        provider_error,
        retry,
    } = action
    {
        if matches!(action.as_ref(), FixtureAction::ProbeFailure { .. }) {
            return Err("probeFailure actions cannot nest".into());
        }
        if expected_error_contains.trim().is_empty() {
            return Err("probeFailure expectedErrorContains must not be empty".into());
        }
        if *retry && provider_error.is_none() {
            return Err(
                "automatic probeFailure retry requires a temporary providerError injection".into(),
            );
        }
        if let Some(f) = provider_error {
            if f.at_decision == 0 || f.message.trim().is_empty() {
                return Err(
                    "providerError requires one-based atDecision and nonempty message".into(),
                );
            }
            if f.player_id.as_ref().is_some_and(|v| v.is_empty())
                || f.request_id.as_ref().is_some_and(|v| v.is_empty())
            {
                return Err(
                    "providerError playerId/requestId must be nonempty when provided".into(),
                );
            }
        }
        Ok(action.as_ref())
    } else {
        Ok(action)
    }
}

pub(super) fn run(
    engine: &mut GameEngine,
    action: &FixtureAction,
    rule: &Value,
    provider: &mut ScriptedProvider,
    expected: &str,
    injection: Option<&ProviderErrorInjection>,
    retry: bool,
) -> Result<Vec<String>, EngineError> {
    let state_before = engine.state().clone();
    let mut clean = retry.then(|| engine.clone());
    let script_before = provider.clone();
    let mut failing = ProbeProvider {
        inner: provider,
        failure: injection,
        visits: vec![],
        injected: false,
    };
    let result = super::action_driver::execute_plain(engine, action, rule, &mut failing);
    let injected = failing.injected;
    let failure_calls = failing.visits.len();
    drop(failing);
    let error = result
        .err()
        .ok_or_else(|| EngineError::new("probeFailure action unexpectedly succeeded"))?;
    if injection.is_some() && !injected {
        return Err(EngineError::new(
            "probeFailure did not reach the configured provider injection",
        ));
    }
    let error_text = error.to_string();
    if !error_text.contains(expected) || injection.is_some_and(|i| !error_text.contains(&i.message))
    {
        return Err(EngineError::new(format!(
            "probeFailure received a different error: {error_text}"
        )));
    }
    if engine.state() != &state_before {
        return Err(EngineError::new(format!(
            "probeFailure did not restore public GameState (stack {} -> {}, events {} -> {})",
            state_before.stack.len(),
            engine.state().stack.len(),
            state_before.events.len(),
            engine.state().events.len()
        )));
    }
    let mut checks = vec![
        format!("expected action error observed: {expected}"),
        "public GameState restored exactly by the engine, without driver rollback".into(),
    ];
    if let Some(injection) = injection {
        checks.push(format!(
            "provider fault reached at local decision {}; observed {failure_calls} callbacks",
            injection.at_decision
        ));
    }
    if let Some(clean) = clean.as_mut() {
        // Replaying scripted answers is a fixture-driver action, never engine
        // restoration. Preserve failed-attempt call accounting for both paths.
        let mut clean_provider = script_before.clone();
        clean_provider.calls = provider.calls;
        provider.decisions = script_before.decisions;
        let mut retried = ProbeProvider {
            inner: provider,
            failure: None,
            visits: vec![],
            injected: false,
        };
        super::action_driver::execute_plain(engine, action, rule, &mut retried)?;
        let retry_visits = std::mem::take(&mut retried.visits);
        drop(retried);
        let mut control = ProbeProvider {
            inner: &mut clean_provider,
            failure: None,
            visits: vec![],
            injected: false,
        };
        super::action_driver::execute_plain(clean, action, rule, &mut control)?;
        let control_visits = std::mem::take(&mut control.visits);
        drop(control);
        if engine.state() != clean.state()
            || provider.decisions != clean_provider.decisions
            || provider.calls != clean_provider.calls
            || retry_visits != control_visits
        {
            return Err(EngineError::new(
                "probeFailure retry differs from clean execution state/decision stream/request trace",
            ));
        }
        checks.push(
            "retry matches clean cloned-engine execution and exact decision request trace".into(),
        );
    }
    Ok(checks)
}

#[cfg(test)]
#[path = "failure_probe_tests.rs"]
mod tests;
