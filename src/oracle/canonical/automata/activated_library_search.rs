use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, BoundCollectionUntapEffectNode, ConditionNode,
    LibrarySearchEffectNode, recognize_activation_cost_sequence,
    recognize_controlled_permanent_count_condition, recognize_controller_library_search,
    recognize_untap_bound_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalSearchResultUntapNode {
    pub condition: ConditionNode,
    pub effect: BoundCollectionUntapEffectNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedLibrarySearchNode {
    pub costs: ActivationCostSequenceNode,
    pub search: LibrarySearchEffectNode,
    pub followup: Option<ConditionalSearchResultUntapNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedLibrarySearchState {
    Costs,
    EffectSeparator,
    Search,
    SearchTerminator,
    FollowupOrComplete,
    FollowupConnector,
    FollowupCondition,
    FollowupEffect,
    FollowupTerminator,
    Complete,
}

#[derive(Default)]
struct ActivatedLibrarySearchFrame {
    costs: Option<ActivationCostSequenceNode>,
    search: Option<LibrarySearchEffectNode>,
    condition: Option<ConditionNode>,
    followup: Option<ConditionalSearchResultUntapNode>,
}

/// Composes a generic activated-cost sequence with the shared controller-owned
/// library-search child. The child owns selectors, result anaphora,
/// destination state, and shuffle sequencing.
pub(in crate::oracle::canonical) fn recognize_activated_library_search(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivatedLibrarySearchNode> {
    let mut state = ActivatedLibrarySearchState::Costs;
    let mut frame = ActivatedLibrarySearchFrame::default();
    loop {
        state = match state {
            ActivatedLibrarySearchState::Costs => {
                cursor.enter("activation.library-search.costs");
                frame.costs = Some(recognize_activation_cost_sequence(cursor)?);
                ActivatedLibrarySearchState::EffectSeparator
            }
            ActivatedLibrarySearchState::EffectSeparator => {
                cursor.enter("activation.library-search.effect-separator");
                cursor.consume(":")?;
                ActivatedLibrarySearchState::Search
            }
            ActivatedLibrarySearchState::Search => {
                cursor.enter("activation.library-search.effect");
                frame.search = Some(recognize_controller_library_search(cursor)?);
                ActivatedLibrarySearchState::SearchTerminator
            }
            ActivatedLibrarySearchState::SearchTerminator => {
                cursor.enter("activation.library-search.terminator");
                cursor.consume(".")?;
                ActivatedLibrarySearchState::FollowupOrComplete
            }
            ActivatedLibrarySearchState::FollowupOrComplete => {
                cursor.enter("activation.library-search.followup-or-complete");
                if cursor.peek().is_some() {
                    ActivatedLibrarySearchState::FollowupConnector
                } else {
                    ActivatedLibrarySearchState::Complete
                }
            }
            ActivatedLibrarySearchState::FollowupConnector => {
                cursor.enter("activation.library-search.followup-connector");
                cursor.consume("then")?;
                ActivatedLibrarySearchState::FollowupCondition
            }
            ActivatedLibrarySearchState::FollowupCondition => {
                cursor.enter("activation.library-search.followup-condition");
                frame.condition = Some(recognize_controlled_permanent_count_condition(cursor)?);
                ActivatedLibrarySearchState::FollowupEffect
            }
            ActivatedLibrarySearchState::FollowupEffect => {
                cursor.enter("activation.library-search.followup-effect");
                let search = frame
                    .search
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a completed library search"))?;
                let effect = recognize_untap_bound_collection(cursor, search.binding)?;
                cursor.require(
                    search.filter.guarantees_card_type(effect.object_type),
                    "a searched-card selector constrained to the referenced permanent type",
                )?;
                frame.followup = Some(ConditionalSearchResultUntapNode {
                    condition: cursor
                        .require_value(frame.condition.take(), "a completed follow-up condition")?,
                    effect,
                });
                ActivatedLibrarySearchState::FollowupTerminator
            }
            ActivatedLibrarySearchState::FollowupTerminator => {
                cursor.enter("activation.library-search.followup-terminator");
                cursor.consume(".")?;
                ActivatedLibrarySearchState::Complete
            }
            ActivatedLibrarySearchState::Complete => {
                cursor.enter("activation.library-search.complete");
                cursor.finish()?;
                return Ok(ActivatedLibrarySearchNode {
                    costs: cursor
                        .require_value(frame.costs, "a populated activation cost sequence")?,
                    search: cursor.require_value(frame.search, "a populated library search")?,
                    followup: frame.followup,
                });
            }
        };
    }
}
