use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, EffectSequenceNode, LocalObjectReference, PlayerPredicate,
    recognize_effect_sequence_with_context,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LibrarySearchEventNode {
    pub player: PlayerPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LibrarySearchTriggeredAbilityNode {
    pub event: LibrarySearchEventNode,
    pub effects: EffectSequenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventState {
    Trigger,
    Quantifier,
    Player,
    Action,
    Possessive,
    Zone,
    Boundary,
    Complete,
}

/// Recognizes the event envelope independently from the triggered effects.
pub(in crate::oracle::canonical) fn recognize_library_search_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchEventNode> {
    let mut state = EventState::Trigger;
    let mut player = None;
    loop {
        state = match state {
            EventState::Trigger => {
                cursor.enter("event.library-search.trigger");
                cursor.consume("whenever")?;
                EventState::Quantifier
            }
            EventState::Quantifier => {
                cursor.enter("event.library-search.quantifier");
                cursor.consume("an")?;
                EventState::Player
            }
            EventState::Player => {
                cursor.enter("event.library-search.player");
                cursor.consume("opponent")?;
                player = Some(PlayerPredicate::Opponent);
                EventState::Action
            }
            EventState::Action => {
                cursor.enter("event.library-search.action");
                cursor.consume("searches")?;
                EventState::Possessive
            }
            EventState::Possessive => {
                cursor.enter("event.library-search.possessive");
                cursor.consume("their")?;
                EventState::Zone
            }
            EventState::Zone => {
                cursor.enter("event.library-search.zone");
                cursor.consume("library")?;
                EventState::Boundary
            }
            EventState::Boundary => {
                cursor.enter("event.library-search.boundary");
                cursor.consume(",")?;
                EventState::Complete
            }
            EventState::Complete => {
                cursor.enter("event.library-search.complete");
                return Ok(LibrarySearchEventNode {
                    player: cursor.require_value(player, "a library-searching player")?,
                });
            }
        };
    }
}

/// The trigger envelope delegates all connector and effect ownership to the
/// shared effect-sequence machine. It contributes only the source context
/// needed to resolve printed-name and `this OBJECT` references.
pub(in crate::oracle::canonical) fn recognize_library_search_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LibrarySearchTriggeredAbilityNode> {
    let event = recognize_library_search_event(cursor)?;
    let context = EffectRecognitionContext {
        source: Some(LocalObjectReference::SourceSelf),
        source_cast_x: false,
    };
    let effects =
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, &context)?;
    cursor.enter("trigger.library-search.complete");
    cursor.finish()?;
    Ok(LibrarySearchTriggeredAbilityNode { event, effects })
}
