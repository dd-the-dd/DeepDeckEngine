use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CompoundObjectConnector, ControlledPermanentChoiceNode, PlayerCompoundExileNode,
    PlayerZoneKind, PossessivePlayerZoneNode, SelectorPredicate, TargetBinding, TargetSelector,
    recognize_target_list, recognize_target_slot_selector,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentState {
    Determiner,
    Predicate,
    Anaphor,
    Relation,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ControlledPermanentAttemptFrame {
    predicate: Option<SelectorPredicate>,
}

/// Recognizes an object chosen by a previously bound player from permanents
/// that player controls. The parent supplies the player binding; this child
/// never guesses an antecedent from reconstructed text.
fn recognize_controlled_permanent_choice(
    cursor: &mut OracleTokenCursor<'_>,
    player: TargetBinding,
) -> MachineResult<ControlledPermanentChoiceNode> {
    let mut state = ControlledPermanentState::Determiner;
    let mut frame = ControlledPermanentAttemptFrame::default();
    loop {
        state = match state {
            ControlledPermanentState::Determiner => {
                cursor.enter("object.controlled-permanent.determiner");
                cursor.require(
                    cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                    }),
                    "`a` or `an`",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                ControlledPermanentState::Predicate
            }
            ControlledPermanentState::Predicate => {
                cursor.enter("object.controlled-permanent.predicate");
                let selector = recognize_target_slot_selector(cursor)?;
                cursor.enter("object.controlled-permanent.predicate");
                let TargetSelector::Object(object_predicate) = selector else {
                    return Err(cursor.fail("a permanent card-type selector"));
                };
                cursor.require(
                    predicate_is_permanent_card_type(&object_predicate),
                    "a permanent card-type selector",
                )?;
                frame.predicate = Some(object_predicate);
                ControlledPermanentState::Anaphor
            }
            ControlledPermanentState::Anaphor => {
                cursor.enter("object.controlled-permanent.player-anaphor");
                let anaphor = cursor.consume_type("referenceWord")?;
                cursor.require(anaphor.raw.eq_ignore_ascii_case("they"), "`they`")?;
                ControlledPermanentState::Relation
            }
            ControlledPermanentState::Relation => {
                cursor.enter("object.controlled-permanent.control-relation");
                let relation = cursor.consume_type("controlWord")?;
                cursor.require(relation.raw.eq_ignore_ascii_case("control"), "`control`")?;
                ControlledPermanentState::Complete
            }
            ControlledPermanentState::Complete => {
                cursor.enter("object.controlled-permanent.complete");
                return Ok(ControlledPermanentChoiceNode {
                    player,
                    predicate: cursor
                        .require_value(frame.predicate, "a populated permanent predicate")?,
                });
            }
        };
    }
}

fn predicate_is_permanent_card_type(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::IsCommander | SelectorPredicate::IsToken => false,
        SelectorPredicate::CardType(card_type) => !matches!(
            card_type,
            super::ObjectCardType::Instant | super::ObjectCardType::Sorcery
        ),
        SelectorPredicate::Intersection(predicates) | SelectorPredicate::Union(predicates) => {
            !predicates.is_empty() && predicates.iter().all(predicate_is_permanent_card_type)
        }
        SelectorPredicate::Not(predicate) => predicate_is_permanent_card_type(predicate),
        SelectorPredicate::Color(_)
        | SelectorPredicate::ColorQuantity(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Status(_)
        | SelectorPredicate::Designation(_)
        | SelectorPredicate::OwnedByAbilityController { .. }
        | SelectorPredicate::PermanentCard
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::NumericComparison { .. } => true,
        SelectorPredicate::ObjectKind(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ChosenCreatureType => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PossessiveZoneState {
    Possessor,
    Zone,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PossessiveZoneAttemptFrame {
    zone: Option<PlayerZoneKind>,
}

fn recognize_possessive_player_zone(
    cursor: &mut OracleTokenCursor<'_>,
    player: TargetBinding,
) -> MachineResult<PossessivePlayerZoneNode> {
    let mut state = PossessiveZoneState::Possessor;
    let mut frame = PossessiveZoneAttemptFrame::default();
    loop {
        state = match state {
            PossessiveZoneState::Possessor => {
                cursor.enter("zone.possessive-player.possessor");
                let possessor = cursor.consume_type("referenceWord")?;
                cursor.require(possessor.raw.eq_ignore_ascii_case("their"), "`their`")?;
                PossessiveZoneState::Zone
            }
            PossessiveZoneState::Zone => {
                cursor.enter("zone.possessive-player.kind");
                let zone_token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("`graveyard`"))?;
                let token = cursor.consume_type("zoneWord")?;
                if !token.raw.eq_ignore_ascii_case("graveyard") {
                    return Err(cursor.fail_with_span(
                        "`graveyard`",
                        zone_token.index,
                        zone_token.span_start,
                        zone_token.span_end,
                        zone_token.raw,
                    ));
                }
                frame.zone = Some(PlayerZoneKind::Graveyard);
                PossessiveZoneState::Complete
            }
            PossessiveZoneState::Complete => {
                cursor.enter("zone.possessive-player.complete");
                return Ok(PossessivePlayerZoneNode {
                    player,
                    zone: cursor.require_value(frame.zone, "a populated player zone")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerCompoundExileState {
    Subject,
    Action,
    Permanent,
    Connector,
    Zone,
    Terminator,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlayerCompoundExileAttemptFrame {
    subject: Option<super::TargetListNode>,
    permanent: Option<ControlledPermanentChoiceNode>,
    zone: Option<PossessivePlayerZoneNode>,
}

/// Recognizes `TARGET_PLAYER exiles CONTROLLED_PERMANENT and POSSESSIVE_ZONE`.
/// The connector is owned by this compound-object list, so lowering may keep
/// the zone changes simultaneous instead of inventing an effect sequence.
pub(in crate::oracle::canonical) fn recognize_player_compound_exile(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerCompoundExileNode> {
    let mut state = PlayerCompoundExileState::Subject;
    let mut frame = PlayerCompoundExileAttemptFrame::default();
    let subject_binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    loop {
        state = match state {
            PlayerCompoundExileState::Subject => {
                cursor.enter("compound-exile.subject");
                let target = recognize_target_list(cursor)?;
                cursor.require(target.slots.len() == 1, "exactly one target player")?;
                cursor.require(
                    matches!(target.slots[0].selector, TargetSelector::Player(_)),
                    "a target player or opponent",
                )?;
                frame.subject = Some(target);
                PlayerCompoundExileState::Action
            }
            PlayerCompoundExileState::Action => {
                cursor.enter("compound-exile.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Exile, VerbForm::ThirdPersonSingular),
                    "`exiles`",
                )?;
                PlayerCompoundExileState::Permanent
            }
            PlayerCompoundExileState::Permanent => {
                cursor.enter("compound-exile.first-object");
                frame.permanent = Some(recognize_controlled_permanent_choice(
                    cursor,
                    subject_binding,
                )?);
                PlayerCompoundExileState::Connector
            }
            PlayerCompoundExileState::Connector => {
                cursor.enter("compound-exile.object-connector");
                let connector_token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("`and`"))?;
                let connector = cursor.consume_type("logicWord")?;
                if !connector.raw.eq_ignore_ascii_case("and") {
                    return Err(cursor.fail_with_span(
                        "`and`",
                        connector_token.index,
                        connector_token.span_start,
                        connector_token.span_end,
                        connector_token.raw,
                    ));
                }
                PlayerCompoundExileState::Zone
            }
            PlayerCompoundExileState::Zone => {
                cursor.enter("compound-exile.second-object");
                frame.zone = Some(recognize_possessive_player_zone(cursor, subject_binding)?);
                PlayerCompoundExileState::Terminator
            }
            PlayerCompoundExileState::Terminator => {
                cursor.enter("compound-exile.terminator");
                cursor.consume(".")?;
                PlayerCompoundExileState::Complete
            }
            PlayerCompoundExileState::Complete => {
                cursor.enter("compound-exile.complete");
                cursor.finish()?;
                return Ok(PlayerCompoundExileNode {
                    subject: cursor.require_value(frame.subject, "a populated targeted subject")?,
                    subject_binding,
                    permanent: cursor
                        .require_value(frame.permanent, "a populated controlled permanent")?,
                    zone: cursor.require_value(frame.zone, "a populated possessive zone")?,
                    connector: CompoundObjectConnector::And,
                });
            }
        };
    }
}
