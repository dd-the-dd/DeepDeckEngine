use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, PlayerZoneKind, recognize_card_noun,
    recognize_positive_card_count,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneActionKind {
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalExactCountZoneActionNode {
    pub player: LocalPlayerReference,
    pub action: ZoneActionKind,
    pub count: NumericAtom,
    pub zone: PlayerZoneKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalZoneActionState {
    Player,
    Modality,
    Action,
    Count,
    Cards,
    From,
    Possessive,
    Zone,
    Complete,
}

/// Recognizes the reusable resolution action
/// `you may exile N cards from your graveyard` without owning either the
/// surrounding trigger or a dependent continuation.
pub(in crate::oracle::canonical) fn recognize_optional_exact_count_zone_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OptionalExactCountZoneActionNode> {
    let mut state = OptionalZoneActionState::Player;
    let mut count = None;
    loop {
        state = match state {
            OptionalZoneActionState::Player => {
                cursor.enter("optional-zone-action.player");
                cursor.consume("you")?;
                OptionalZoneActionState::Modality
            }
            OptionalZoneActionState::Modality => {
                cursor.enter("optional-zone-action.modality");
                cursor.consume("may")?;
                OptionalZoneActionState::Action
            }
            OptionalZoneActionState::Action => {
                cursor.enter("optional-zone-action.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Exile, VerbForm::Base),
                    "the `exile` zone action",
                )?;
                OptionalZoneActionState::Count
            }
            OptionalZoneActionState::Count => {
                cursor.enter("optional-zone-action.count");
                count = Some(recognize_positive_card_count(cursor)?);
                cursor.require(
                    matches!(count, Some(NumericAtom::Integer(_))),
                    "an exact literal card count",
                )?;
                OptionalZoneActionState::Cards
            }
            OptionalZoneActionState::Cards => {
                cursor.enter("optional-zone-action.cards");
                recognize_card_noun(
                    cursor,
                    count
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a populated exact card count"))?,
                )?;
                OptionalZoneActionState::From
            }
            OptionalZoneActionState::From => {
                cursor.enter("optional-zone-action.from");
                cursor.consume("from")?;
                OptionalZoneActionState::Possessive
            }
            OptionalZoneActionState::Possessive => {
                cursor.enter("optional-zone-action.possessive");
                cursor.consume("your")?;
                OptionalZoneActionState::Zone
            }
            OptionalZoneActionState::Zone => {
                cursor.enter("optional-zone-action.zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(
                    zone.raw.eq_ignore_ascii_case("graveyard"),
                    "the controller's graveyard",
                )?;
                OptionalZoneActionState::Complete
            }
            OptionalZoneActionState::Complete => {
                cursor.enter("optional-zone-action.complete");
                return Ok(OptionalExactCountZoneActionNode {
                    player: LocalPlayerReference::Controller,
                    action: ZoneActionKind::Exile,
                    count: cursor.require_value(count, "a populated exact card count")?,
                    zone: PlayerZoneKind::Graveyard,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IfPerformedState {
    If,
    Player,
    Performed,
    Separator,
    Complete,
}

/// Recognizes only the dependent-effect connector `If you do,`. The parent
/// binds it to a preceding optional action and delegates the following effect
/// to an independent child automaton.
pub(in crate::oracle::canonical) fn recognize_if_performed_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    let mut state = IfPerformedState::If;
    loop {
        state = match state {
            IfPerformedState::If => {
                cursor.enter("if-performed-state.if");
                cursor.consume("if")?;
                IfPerformedState::Player
            }
            IfPerformedState::Player => {
                cursor.enter("if-performed-state.player");
                cursor.consume("you")?;
                IfPerformedState::Performed
            }
            IfPerformedState::Performed => {
                cursor.enter("if-performed-state.performed");
                cursor.consume("do")?;
                IfPerformedState::Separator
            }
            IfPerformedState::Separator => {
                cursor.enter("if-performed.separator");
                cursor.consume(",")?;
                IfPerformedState::Complete
            }
            IfPerformedState::Complete => {
                cursor.enter("if-performed.complete");
                return Ok(());
            }
        };
    }
}
