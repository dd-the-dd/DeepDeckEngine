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
    /// A static grant of a rule whose kind has no layer-6 grant the engine
    /// reads (granted activated, mana, triggered, static and keyword rules
    /// each have one; any other rule kind stops here).
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
    /// An event-history count filtered by a permanent state (face-down,
    /// tapped, a designation or counters): the event log records the
    /// object's definition, not its state.
    EventHistoryState,
    /// A modal choice that allows zero modes (`choose up to N`, `any
    /// number`): the runtime declares at least one mode.
    ZeroModeChoice,
}

impl EngineGap {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 9] = [
        Self::EventMatcher,
        Self::GrantedNonActivatedAbility,
        Self::MeasuredPerformance,
        Self::SourceScopedPrevention,
        Self::PermissionWindow,
        Self::ResolutionCostAction,
        Self::ActivationCostAction,
        Self::EventHistoryState,
        Self::ZeroModeChoice,
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
            Self::EventHistoryState => "engine.gap.event-history-state",
            Self::ZeroModeChoice => "engine.gap.zero-mode-choice",
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
