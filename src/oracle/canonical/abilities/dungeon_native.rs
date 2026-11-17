use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// A labeled line of a Dungeon face (CR 309.4). Other Dungeon abilities, such
/// as an entry restriction, keep their ordinary static grammar.
pub(in crate::oracle::canonical) fn claims_dungeon_room_family(input: &AbilityInput<'_>) -> bool {
    dungeon_face(input.face_type_line) && dungeon_room_label_width(input.tokens).is_some()
}

pub(in crate::oracle::canonical) fn claims_dungeon_entry_restriction(
    input: &AbilityInput<'_>,
) -> bool {
    dungeon_face(input.face_type_line) && dungeon_entry_restriction_starts(input.tokens)
}

/// A static ability of the Dungeon card itself, active in the command zone
/// where a Dungeon is put (CR 309.2), never on the battlefield.
pub(in crate::oracle::canonical) fn parse_dungeon_entry_restriction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("dungeon-entry-restriction", input.tokens);
    let node = recognize_dungeon_entry_restriction(&mut cursor, &input.source.text)?;
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": { "kind": "inZone", "object": self_ref(), "zone": { "kind": "command" } },
            "modifiers": [{
                "kind": "dungeonEntryRestriction",
                "procedure": format!("venture into {}", node.venture_into),
            }],
        }),
        &[
            "Read the entry prohibition of this Dungeon",
            "Type the quoted venture procedure that names it",
        ],
    ))
}

/// CR 309.4c: entering a room triggers its room ability. The room body is the
/// shared imperative program; the engine reads `room`, `nextRooms`, `effects`
/// and the optional target declaration of that program.
pub(in crate::oracle::canonical) fn parse_dungeon_room(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("dungeon-room", input.tokens);
    let room = recognize_dungeon_room(&mut cursor, &input.source.text, |cursor| {
        recognize_and_lower_imperative_program(cursor, input)
    })?;
    let (effects, declaration) = room.body;
    let mut rule = json!({
        "kind": "dungeonRoom",
        "room": room.room,
        "nextRooms": room.next_rooms,
        "effects": effects,
    });
    if let Some(declaration) = declaration {
        rule["declaration"] = declaration;
    }
    Ok(draft(
        rule,
        &[
            "Read the room label and its em dash",
            "Compile the room ability through the shared imperative program",
            "Preserve the typed `Leads to` room graph",
        ],
    ))
}

#[cfg(test)]
#[path = "dungeon_native_tests.rs"]
mod tests;
