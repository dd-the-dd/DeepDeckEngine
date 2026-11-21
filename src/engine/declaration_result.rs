//! Declaration grammar rejection and dependency failure have different roles.
//! A source-budget error must never masquerade as an impossible declaration.
use super::*;

pub(super) enum DeclarationReadFailure<E> {
    GrammarRejected,
    Dependency(E),
    Capture(EngineError),
    Evaluation(EngineError),
}

#[derive(Debug)]
pub(super) enum DeclarationDependencyFailure<E> {
    Dependency(E),
    Capture(EngineError),
    Evaluation(EngineError),
}

impl DeclarationDependencyFailure<EngineError> {
    pub(super) fn into_engine_error(self) -> EngineError {
        match self {
            Self::Dependency(error) | Self::Capture(error) | Self::Evaluation(error) => error,
        }
    }
}

/// Only the historical Option adapter has no error channel. Checked consumers
/// retain both dependencies and capture failures through the typed result.
pub(super) fn legacy_declarations(
    result: Result<
        Option<Vec<CastingDeclaration>>,
        DeclarationDependencyFailure<std::convert::Infallible>,
    >,
) -> Option<Vec<CastingDeclaration>> {
    match result {
        Ok(value) => value,
        Err(DeclarationDependencyFailure::Dependency(never)) => match never {},
        Err(
            DeclarationDependencyFailure::Capture(_) | DeclarationDependencyFailure::Evaluation(_),
        ) => None,
    }
}

pub(super) fn finish<T, E>(
    result: Result<T, DeclarationReadFailure<E>>,
) -> Result<Option<T>, DeclarationDependencyFailure<E>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(DeclarationReadFailure::GrammarRejected) => Ok(None),
        Err(DeclarationReadFailure::Dependency(error)) => {
            Err(DeclarationDependencyFailure::Dependency(error))
        }
        Err(DeclarationReadFailure::Capture(error)) => {
            Err(DeclarationDependencyFailure::Capture(error))
        }
        Err(DeclarationReadFailure::Evaluation(error)) => {
            Err(DeclarationDependencyFailure::Evaluation(error))
        }
    }
}

impl GameEngine {
    /// Real production, declaration and payment readers, sharing one immutable
    /// state frame. No bounded-test declaration callback is used by this path.
    pub(super) fn complete_checked_mana_activation_actions(
        &self,
        player_index: usize,
    ) -> Result<Vec<LegalAction>, EngineError> {
        self.try_checked_mana_activation_actions(
            player_index,
            &mut |rule, controller, source, context| {
                self.try_declarations_for_activated_rule_with_budget(
                    rule,
                    controller,
                    source,
                    &mut |budget_controller, spending| {
                        self.checked_mana_budget_in_context(budget_controller, spending, context)
                    },
                )
                .map_err(DeclarationDependencyFailure::into_engine_error)
            },
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4")
))]
#[path = "declaration_result_tests.rs"]
mod tests;
