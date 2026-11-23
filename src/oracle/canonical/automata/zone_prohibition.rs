use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, ObjectKind, ObjectQueryNode, PlayerSetReferenceNode, SpellCastQueryNode,
    recognize_card_zone_list, recognize_object_query, recognize_player_set_reference,
    recognize_spell_cast_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneProhibitionNode {
    Cast {
        players: PlayerSetReferenceNode,
        spells: SpellCastQueryNode,
        zones: Vec<CardZoneName>,
    },
    Enter {
        cards: ObjectQueryNode,
        zones: Vec<CardZoneName>,
    },
}
pub(in crate::oracle::canonical) enum ZoneProhibitionAttempt {
    Parsed(ZoneProhibitionNode),
    NotClaimed,
    UnsupportedEnvelope(MachineFailure),
    Failed(MachineFailure),
}
#[derive(Clone, Copy)]
enum State {
    Subject,
    Prohibition,
    Action,
    Zones,
    Entry,
    Terminator,
    Complete,
}
#[derive(Default)]
struct Frame {
    players: Option<PlayerSetReferenceNode>,
    cards: Option<ObjectQueryNode>,
    spells: Option<SpellCastQueryNode>,
    zones: Option<Vec<CardZoneName>>,
    committed: bool,
}

fn consume_prohibition(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("static.zone-prohibition.prohibition");
    let word = cursor
        .peek()
        .filter(|w| w.eq_ignore_ascii_case("can't") || w.eq_ignore_ascii_case("cannot"))
        .ok_or_else(|| cursor.fail("can't or cannot"))?
        .to_owned();
    cursor.consume(&word)
}
pub(in crate::oracle::canonical) fn try_recognize_zone_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> ZoneProhibitionAttempt {
    let checkpoint = cursor.checkpoint();
    let mut state = State::Subject;
    let mut frame = Frame::default();
    let result: MachineResult<ZoneProhibitionNode> = (|| loop {
        state = match state {
            State::Subject => {
                cursor.enter("static.zone-prohibition.subject");
                if cursor.peek().is_some_and(|w| {
                    w.eq_ignore_ascii_case("players")
                        || w.eq_ignore_ascii_case("each")
                        || w.eq_ignore_ascii_case("your")
                }) {
                    frame.players = Some(recognize_player_set_reference(cursor)?);
                    State::Prohibition
                } else {
                    let cards = recognize_object_query(cursor)?;
                    cursor.enter("static.zone-prohibition.card-domain");
                    cursor.require(
                        cards.predicate.is_in_domain(ObjectKind::Card),
                        "a collection explicitly anchored in the card domain",
                    )?;
                    frame.cards = Some(cards);
                    cursor.consume("in")?;
                    frame.zones = Some(recognize_card_zone_list(cursor)?);
                    State::Prohibition
                }
            }
            State::Prohibition => {
                consume_prohibition(cursor)?;
                State::Action
            }
            State::Action => {
                cursor.enter("static.zone-prohibition.action");
                if frame.players.is_some() {
                    cursor.consume("cast")?;
                    frame.spells = Some(recognize_spell_cast_collection(cursor)?);
                    cursor.consume("from")?;
                    frame.committed = true;
                    State::Zones
                } else {
                    cursor.consume("enter")?;
                    frame.committed = true;
                    State::Entry
                }
            }
            State::Zones => {
                frame.zones = Some(recognize_card_zone_list(cursor)?);
                State::Terminator
            }
            State::Entry => {
                cursor.enter("static.zone-prohibition.destination");
                cursor.consume_phrase(&["the", "battlefield"])?;
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("static.zone-prohibition.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                let zones = cursor.require_value(frame.zones.take(), "source zones")?;
                return if let Some(players) = frame.players {
                    Ok(ZoneProhibitionNode::Cast {
                        players,
                        spells: cursor.require_value(frame.spells.take(), "spell query")?,
                        zones,
                    })
                } else {
                    Ok(ZoneProhibitionNode::Enter {
                        cards: cursor.require_value(frame.cards.take(), "card query")?,
                        zones,
                    })
                };
            }
        }
    })();
    match result {
        Ok(node) if cursor.peek().is_none() => ZoneProhibitionAttempt::Parsed(node),
        Ok(_) => {
            let failure =
                cursor.fail("one complete atomic zone prohibition without a later clause");
            cursor.rollback(checkpoint);
            ZoneProhibitionAttempt::UnsupportedEnvelope(failure)
        }
        Err(_) if !frame.committed => {
            cursor.rollback(checkpoint);
            ZoneProhibitionAttempt::NotClaimed
        }
        Err(failure) if matches!(state, State::Terminator) => {
            cursor.rollback(checkpoint);
            ZoneProhibitionAttempt::UnsupportedEnvelope(failure)
        }
        Err(failure) => ZoneProhibitionAttempt::Failed(failure),
    }
}

#[cfg(test)]
#[path = "zone_prohibition_tests.rs"]
mod tests;
