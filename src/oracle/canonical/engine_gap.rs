//! Engine gaps: a complete typed reading that the runtime cannot execute.
//!
//! A grammar failure says the parser did not read the words. An engine gap
//! says it did: the ability reached complete typed IR, and the missing piece
//! is a runtime primitive, matcher or payment schema. The two must never share
//! a state name, otherwise the engine backlog hides inside grammar rankings
//! (roadmap S0: "engine gaps become an explicit `engine.gap.<kind>` state").
//!
//! A producer keeps entering its own frame state before it fails, so the trace
//! still records where the gap was found; the reported state is the gap kind.

/// The runtime capability whose absence stops an otherwise complete reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EngineGap {
    /// An event pattern the runtime matcher cannot evaluate (a modifier,
    /// counterpart predicate or self matcher without a runtime observer).
    EventMatcher,
    /// A static grant of a triggered or static ability: the engine only
    /// observes granted activated and mana abilities.
    GrantedNonActivatedAbility,
    /// A reflexive or optional program whose instruction performance the
    /// runtime does not measure (CR 603.12, 608.2c "if you do").
    MeasuredPerformance,
    /// A static damage prevention scoped by source characteristics that the
    /// runtime does not evaluate.
    SourceScopedPrevention,
    /// A casting or play permission window the runtime cannot represent.
    PermissionWindow,
    /// A typed payment action that has no resolution-cost program.
    ResolutionCostAction,
    /// A typed payment action that has no activation-cost schema.
    ActivationCostAction,
}

impl EngineGap {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 7] = [
        Self::EventMatcher,
        Self::GrantedNonActivatedAbility,
        Self::MeasuredPerformance,
        Self::SourceScopedPrevention,
        Self::PermissionWindow,
        Self::ResolutionCostAction,
        Self::ActivationCostAction,
    ];

    /// The reported failure state, always `engine.gap.<kind>`.
    pub(crate) const fn state(self) -> &'static str {
        match self {
            Self::EventMatcher => "engine.gap.event-matcher",
            Self::GrantedNonActivatedAbility => "engine.gap.granted-nonactivated-ability",
            Self::MeasuredPerformance => "engine.gap.measured-performance",
            Self::SourceScopedPrevention => "engine.gap.source-scoped-prevention",
            Self::PermissionWindow => "engine.gap.permission-window",
            Self::ResolutionCostAction => "engine.gap.resolution-cost-action",
            Self::ActivationCostAction => "engine.gap.activation-cost-action",
        }
    }
}

/// True for every state reserved to engine gaps. Grammar producers never use
/// this prefix; `engine_gap_tests.rs` enforces the reverse direction.
pub(crate) fn is_engine_gap_state(state: &str) -> bool {
    state.starts_with("engine.gap.")
}

#[cfg(test)]
#[path = "engine_gap_tests.rs"]
mod tests;
