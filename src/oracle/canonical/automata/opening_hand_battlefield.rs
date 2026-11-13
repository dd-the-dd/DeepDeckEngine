use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, recognize_source_reference, recognize_source_reference_with_antecedent,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OpeningHandBattlefieldNode {
    pub source: LocalObjectReference,
    pub entering_object: LocalObjectReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpeningHandBattlefieldState {
    Conditional,
    Source,
    SourceState,
    HandRelation,
    HandOwner,
    OpeningQualifier,
    HandZone,
    Separator,
    Player,
    Modality,
    Begin,
    GameArticle,
    GameScope,
    ObjectRelation,
    EnteringObject,
    DestinationRelation,
    DestinationArticle,
    DestinationZone,
    Terminator,
    Complete,
}

#[derive(Default)]
struct OpeningHandBattlefieldFrame {
    source: Option<LocalObjectReference>,
    entering_object: Option<LocalObjectReference>,
}

/// Recognizes the optional pregame action that moves the current source from
/// its controller's opening hand to the battlefield. The source name and the
/// later pronoun are resolved by the shared reference machine; this automaton
/// owns only the opening-hand/pregame envelope.
pub(in crate::oracle::canonical) fn recognize_opening_hand_battlefield(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<OpeningHandBattlefieldNode> {
    let mut state = OpeningHandBattlefieldState::Conditional;
    let mut frame = OpeningHandBattlefieldFrame::default();

    loop {
        state = match state {
            OpeningHandBattlefieldState::Conditional => {
                cursor.enter("pregame.opening-hand.conditional");
                let conditional = cursor.consume_type("logicWord")?;
                cursor.require(
                    conditional.raw.eq_ignore_ascii_case("if"),
                    "the `if` conditional",
                )?;
                OpeningHandBattlefieldState::Source
            }
            OpeningHandBattlefieldState::Source => {
                cursor.enter("pregame.opening-hand.source");
                frame.source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                OpeningHandBattlefieldState::SourceState
            }
            OpeningHandBattlefieldState::SourceState => {
                cursor.enter("pregame.opening-hand.source-state");
                cursor.consume("is")?;
                OpeningHandBattlefieldState::HandRelation
            }
            OpeningHandBattlefieldState::HandRelation => {
                cursor.enter("pregame.opening-hand.hand-relation");
                cursor.consume("in")?;
                OpeningHandBattlefieldState::HandOwner
            }
            OpeningHandBattlefieldState::HandOwner => {
                cursor.enter("pregame.opening-hand.hand-owner");
                cursor.consume("your")?;
                OpeningHandBattlefieldState::OpeningQualifier
            }
            OpeningHandBattlefieldState::OpeningQualifier => {
                cursor.enter("pregame.opening-hand.opening-qualifier");
                cursor.consume("opening")?;
                OpeningHandBattlefieldState::HandZone
            }
            OpeningHandBattlefieldState::HandZone => {
                cursor.enter("pregame.opening-hand.hand-zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(zone.raw.eq_ignore_ascii_case("hand"), "the hand zone")?;
                OpeningHandBattlefieldState::Separator
            }
            OpeningHandBattlefieldState::Separator => {
                cursor.enter("pregame.opening-hand.separator");
                cursor.consume(",")?;
                OpeningHandBattlefieldState::Player
            }
            OpeningHandBattlefieldState::Player => {
                cursor.enter("pregame.opening-hand.player");
                cursor.consume("you")?;
                OpeningHandBattlefieldState::Modality
            }
            OpeningHandBattlefieldState::Modality => {
                cursor.enter("pregame.opening-hand.modality");
                let modality = cursor.consume_type("modalityWord")?;
                cursor.require(
                    modality.raw.eq_ignore_ascii_case("may"),
                    "the optional `may` modality",
                )?;
                OpeningHandBattlefieldState::Begin
            }
            OpeningHandBattlefieldState::Begin => {
                cursor.enter("pregame.opening-hand.begin");
                cursor.consume("begin")?;
                OpeningHandBattlefieldState::GameArticle
            }
            OpeningHandBattlefieldState::GameArticle => {
                cursor.enter("pregame.opening-hand.game-article");
                cursor.consume("the")?;
                OpeningHandBattlefieldState::GameScope
            }
            OpeningHandBattlefieldState::GameScope => {
                cursor.enter("pregame.opening-hand.game-scope");
                cursor.consume("game")?;
                OpeningHandBattlefieldState::ObjectRelation
            }
            OpeningHandBattlefieldState::ObjectRelation => {
                cursor.enter("pregame.opening-hand.object-relation");
                cursor.consume("with")?;
                OpeningHandBattlefieldState::EnteringObject
            }
            OpeningHandBattlefieldState::EnteringObject => {
                cursor.enter("pregame.opening-hand.entering-object");
                frame.entering_object = Some(recognize_source_reference_with_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    frame.source.as_ref(),
                )?);
                OpeningHandBattlefieldState::DestinationRelation
            }
            OpeningHandBattlefieldState::DestinationRelation => {
                cursor.enter("pregame.opening-hand.destination-relation");
                cursor.consume("on")?;
                OpeningHandBattlefieldState::DestinationArticle
            }
            OpeningHandBattlefieldState::DestinationArticle => {
                cursor.enter("pregame.opening-hand.destination-article");
                cursor.consume("the")?;
                OpeningHandBattlefieldState::DestinationZone
            }
            OpeningHandBattlefieldState::DestinationZone => {
                cursor.enter("pregame.opening-hand.destination-zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(
                    zone.raw.eq_ignore_ascii_case("battlefield"),
                    "the battlefield zone",
                )?;
                OpeningHandBattlefieldState::Terminator
            }
            OpeningHandBattlefieldState::Terminator => {
                cursor.enter("pregame.opening-hand.terminator");
                cursor.consume(".")?;
                OpeningHandBattlefieldState::Complete
            }
            OpeningHandBattlefieldState::Complete => {
                cursor.enter("pregame.opening-hand.complete");
                cursor.finish()?;
                return Ok(OpeningHandBattlefieldNode {
                    source: cursor.require_value(frame.source, "a resolved source reference")?,
                    entering_object: cursor
                        .require_value(frame.entering_object, "a resolved entering object")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "opening_hand_battlefield_tests.rs"]
mod tests;
