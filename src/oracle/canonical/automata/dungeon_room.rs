use crate::oracle::model::RecognizedEntity;

use super::super::{MachineResult, OracleTokenCursor};

/// CR 309.4: a Dungeon room is a labeled room ability. The label names the
/// room; the body is an ordinary effect program owned by the caller; the
/// trailing `(Leads to: A, B)` is the room graph (CR 309.5a), not a reminder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DungeonRoomNode<T> {
    pub room: String,
    pub body: T,
    pub next_rooms: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DungeonRoomState {
    Label,
    LabelSeparator,
    Body,
    Routes,
    Complete,
}

fn is_room_dash(token: &RecognizedEntity) -> bool {
    token.raw == "\u{2014}"
}

/// A room or route name word. Structural punctuation never belongs to a name.
fn is_room_name_word(token: &RecognizedEntity) -> bool {
    !matches!(
        token.raw.as_str(),
        "\u{2014}" | "." | "," | ":" | ";" | "(" | ")" | "\"" | "\u{2022}"
    ) && token.raw.chars().any(char::is_alphanumeric)
        || token.raw == "!"
}

/// A Dungeon card type before the type-line dash (CR 309.1).
pub(in crate::oracle::canonical) fn dungeon_face(type_line: &str) -> bool {
    type_line
        .split('\u{2014}')
        .next()
        .unwrap_or(type_line)
        .split(|character: char| !character.is_alphabetic())
        .any(|word| word == "Dungeon")
}

/// Structural claim: one or more name words, then the room em dash.
pub(in crate::oracle::canonical) fn dungeon_room_label_width(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    let dash = tokens.iter().position(is_room_dash)?;
    (dash > 0 && tokens[..dash].iter().all(is_room_name_word)).then_some(dash)
}

/// The position of the route parenthetical when it closes the ability.
fn routes_start(tokens: &[RecognizedEntity], body_start: usize) -> Option<usize> {
    let open = tokens.iter().rposition(|token| token.raw == "(")?;
    let header = ["(", "leads", "to", ":"];
    (open >= body_start
        && tokens.last().is_some_and(|token| token.raw == ")")
        && tokens.len() >= open + header.len()
        && tokens[open..open + header.len()]
            .iter()
            .zip(header)
            .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected)))
    .then_some(open)
}

fn name_text(text: &str, words: &[RecognizedEntity]) -> Option<String> {
    let first = words.first()?;
    let last = words.last()?;
    text.get(first.span_start..last.span_end)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

/// `You can't enter this dungeon unless you "venture into NAME."` (CR 309.3,
/// 701.49c): the only procedure that may enter this Dungeon is the quoted
/// keyword action naming it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DungeonEntryRestrictionNode {
    /// The dungeon named by `venture into NAME`.
    pub venture_into: String,
}

pub(in crate::oracle::canonical) fn dungeon_entry_restriction_starts(
    tokens: &[RecognizedEntity],
) -> bool {
    let words = ["you", "can't", "enter", "this", "dungeon", "unless"];
    tokens.len() > words.len()
        && tokens
            .iter()
            .zip(words)
            .all(|(token, word)| token.raw.eq_ignore_ascii_case(word))
}

pub(in crate::oracle::canonical) fn recognize_dungeon_entry_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    text: &str,
) -> MachineResult<DungeonEntryRestrictionNode> {
    cursor.enter("dungeon-entry.restriction");
    cursor.consume_phrase(&["you", "can't", "enter"])?;
    cursor.enter("dungeon-entry.object");
    cursor.consume_phrase(&["this", "dungeon"])?;
    cursor.enter("dungeon-entry.procedure");
    cursor.consume_phrase(&["unless", "you", "\""])?;
    cursor.consume_phrase(&["venture", "into"])?;
    cursor.enter("dungeon-entry.dungeon-name");
    let start = cursor.position();
    while cursor.peek_token().is_some_and(is_room_name_word) {
        cursor.advance_to(cursor.position() + 1)?;
    }
    let name = name_text(text, cursor.token_slice(start, cursor.position()));
    let venture_into = cursor.require_value(name, "the name of the Dungeon to venture into")?;
    cursor.require(
        !venture_into.eq_ignore_ascii_case("the dungeon"),
        "a named Dungeon procedure (`venture into the dungeon` enters any Dungeon)",
    )?;
    cursor.enter("dungeon-entry.quote-end");
    cursor.consume_phrase(&[".", "\""])?;
    if cursor.peek() == Some(".") {
        cursor.consume(".")?;
    }
    cursor.finish()?;
    Ok(DungeonEntryRestrictionNode { venture_into })
}

/// Recognizes `ROOM — BODY [(Leads to: ROOM, ROOM)]`. The body child runs on a
/// bounded view and must consume all of it; names are read from token spans.
pub(in crate::oracle::canonical) fn recognize_dungeon_room<T>(
    cursor: &mut OracleTokenCursor<'_>,
    text: &str,
    body: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<DungeonRoomNode<T>> {
    let mut state = DungeonRoomState::Label;
    let mut room = None;
    let mut body = Some(body);
    let mut parsed_body = None;
    let mut next_rooms = Vec::new();
    loop {
        state = match state {
            DungeonRoomState::Label => {
                cursor.enter("dungeon-room.label");
                let start = cursor.position();
                let width = dungeon_room_label_width(cursor.token_slice(start, usize::MAX))
                    .ok_or_else(|| cursor.fail("a room name followed by an em dash"))?;
                room = name_text(text, cursor.token_slice(start, start + width));
                cursor.advance_to(start + width)?;
                DungeonRoomState::LabelSeparator
            }
            DungeonRoomState::LabelSeparator => {
                cursor.enter("dungeon-room.label-separator");
                cursor.consume("\u{2014}")?;
                DungeonRoomState::Body
            }
            DungeonRoomState::Body => {
                cursor.enter("dungeon-room.body");
                let all = cursor.token_slice(0, usize::MAX);
                let end = routes_start(all, cursor.position()).unwrap_or(all.len());
                cursor.require(end > cursor.position(), "a room effect before its routes")?;
                // A malformed route list is never skipped as reminder text.
                cursor.require(
                    !cursor
                        .token_slice(cursor.position(), end)
                        .windows(2)
                        .any(|pair| {
                            pair[0].raw == "(" && pair[1].raw.eq_ignore_ascii_case("leads")
                        }),
                    "a room graph written `(Leads to: ROOM, ROOM)` at the end of the room",
                )?;
                let child = body.take().expect("the room body is read once");
                parsed_body = Some(cursor.bounded(end, child)?);
                if end < all.len() {
                    DungeonRoomState::Routes
                } else {
                    DungeonRoomState::Complete
                }
            }
            DungeonRoomState::Routes => {
                cursor.enter("dungeon-room.routes");
                cursor.consume_phrase(&["(", "leads", "to", ":"])?;
                loop {
                    cursor.enter("dungeon-room.route-name");
                    let start = cursor.position();
                    while cursor
                        .peek_token()
                        .is_some_and(|token| token.raw != "," && token.raw != ")")
                    {
                        let word = cursor.peek_token().expect("checked token");
                        cursor.require(is_room_name_word(word), "a room name word")?;
                        cursor.advance_to(cursor.position() + 1)?;
                    }
                    let name = name_text(text, cursor.token_slice(start, cursor.position()));
                    next_rooms.push(cursor.require_value(name, "a nonempty room name")?);
                    if cursor.peek() == Some(",") {
                        cursor.consume(",")?;
                    } else {
                        cursor.consume(")")?;
                        break;
                    }
                }
                DungeonRoomState::Complete
            }
            DungeonRoomState::Complete => {
                cursor.enter("dungeon-room.complete");
                cursor.finish()?;
                return Ok(DungeonRoomNode {
                    room: cursor.require_value(room, "a nonempty room name")?,
                    body: cursor.require_value(parsed_body, "a populated room body")?,
                    next_rooms,
                });
            }
        };
    }
}
