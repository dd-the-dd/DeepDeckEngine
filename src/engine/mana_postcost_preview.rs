//! Read-only execution of a fully declared cost prefix on a shadow engine.
//! Pure literal/tap production never constructs this shadow. All provider
//! callbacks are rejected: a preview is not permission to choose option zero.
use super::*;

#[derive(Default)]
pub(super) struct UndeclaredPreviewChoice {
    requested: Option<String>,
}

pub(super) enum PostcostManaPreview {
    Complete(Vec<ManaChoice>),
    /// A real cost/entry decision is necessary. This is an unresolved explicit
    /// declaration, never a certified mana output and never an auto-payment.
    DeferredNeedsProvider {
        request_id: String,
    },
}

impl UndeclaredPreviewChoice {
    fn refuse(&mut self, request: &EngineDecisionRequest) -> EngineError {
        self.requested = Some(request.id.clone());
        preview_choice_error(request)
    }
}
fn preview_choice_error(request: &EngineDecisionRequest) -> EngineError {
    EngineError::new(format!(
        "unsupported mana cost preview: undeclared decision {}",
        request.id,
    ))
}
impl DecisionProvider for UndeclaredPreviewChoice {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(self.refuse(request))
    }
    fn choose_number(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        Err(self.refuse(request))
    }
    fn choose_card_instance_ids(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        Err(self.refuse(request))
    }
    fn choose_card_name(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        Err(self.refuse(request))
    }
    fn choose_loop_shortcut_policy(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(self.refuse(request))
    }
}

impl GameEngine {
    pub(super) fn postcost_choices_for_declared_action(
        &self,
        action: &LegalAction,
        declared: &ManaChoice,
    ) -> Result<PostcostManaPreview, EngineError> {
        let source_id = action
            .card_instance_id
            .as_deref()
            .ok_or_else(|| EngineError::new("mana preview has no declared source"))?;
        let (p, c) = self
            .permanent_position(source_id)
            .ok_or_else(|| EngineError::new("mana preview source is absent"))?;
        let source = self.state.players[p].battlefield[c].clone();
        let rule = declared.program(&source);
        if !self.mana_production_requires_postcost(rule) {
            return Ok(PostcostManaPreview::Complete(vec![declared.clone()]));
        }
        let player_index = self.player_index(&action.player_id)?;
        let has_tap_cost = rule.is_none_or(|rule| {
            rule["costs"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|cost| value_kind(cost) == Some("tap"))
        });
        let mut shadow = self.clone();
        let mut provider = UndeclaredPreviewChoice::default();
        let mut project = |engine: &mut GameEngine, context: Option<usize>| {
            engine.apply_declared_mana_activation_costs(
                mana_activation_costs::ManaActivationCostRequest {
                    player_index,
                    action,
                    source_instance_id: source_id,
                    rule,
                    choice: declared,
                    has_tap_cost,
                },
                &mut provider,
            )?;
            let runtime_source = match context {
                Some(index) => engine.immediate_source_context(index)?,
                None => mana_postcost::immediate_runtime_source(action, &source),
            };
            engine.resolve_postcost_mana_choices(rule, declared, &runtime_source)
        };
        let result = if rule.is_some_and(mana_postcost::source_history_required) {
            shadow.with_immediate_source_context(action, &source, |engine, context| {
                project(engine, Some(context))
            })
        } else {
            project(&mut shadow, None)
        };
        // This state is written only by a DecisionProvider callback. Never
        // infer semantic deferral from a diagnostic's text or disguise a real
        // numeric/context error before a requested choice as deferral.
        if let Some(request_id) = provider.requested {
            return Ok(PostcostManaPreview::DeferredNeedsProvider { request_id });
        }
        result.map(PostcostManaPreview::Complete)
    }
}
