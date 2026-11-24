//! Locations form a union whether Oracle joins them with `and`, `or` or commas.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneName {
    Graveyard,
    Library,
    Exile,
    Hand,
    Battlefield,
}
impl CardZoneName {
    pub fn ir_name(self) -> &'static str {
        match self {
            Self::Graveyard => "graveyard",
            Self::Library => "library",
            Self::Exile => "exile",
            Self::Hand => "hand",
            Self::Battlefield => "battlefield",
        }
    }
}

#[derive(Clone, Copy)]
enum State {
    Zone,
    Connector,
    AfterComma,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_card_zone_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<CardZoneName>> {
    let mut state = State::Zone;
    let mut zones = Vec::new();
    loop {
        state = match state {
            State::Zone => {
                cursor.enter("zone-list.zone");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("battlefield"))
                {
                    return Err(
                        cursor.fail("graveyard, library, exile or hand (singular or plural)")
                    );
                }
                let zone = recognize_card_zone_atom(cursor)?;
                if !zones.contains(&zone) {
                    zones.push(zone);
                }
                State::Connector
            }
            State::Connector => {
                cursor.enter("zone-list.connector-or-complete");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    // A comma continues the list only when a connector or
                    // another zone follows; otherwise it is the parent's
                    // boundary (`cards in your library, rounded up`).
                    Some(",") if comma_continues_zone_list(cursor.peek_ahead(1)) => {
                        cursor.consume(",")?;
                        State::AfterComma
                    }
                    Some("and" | "or" | "and/or") => {
                        let word = cursor.peek().unwrap().to_owned();
                        cursor.consume(&word)?;
                        State::Zone
                    }
                    _ => State::Complete,
                }
            }
            State::AfterComma => {
                cursor.enter("zone-list.after-comma");
                if cursor.peek().is_some_and(|w| {
                    w.eq_ignore_ascii_case("and")
                        || w.eq_ignore_ascii_case("or")
                        || w.eq_ignore_ascii_case("and/or")
                }) {
                    let word = cursor.peek().unwrap().to_owned();
                    cursor.consume(&word)?;
                }
                State::Zone
            }
            State::Complete => {
                cursor.enter("zone-list.complete");
                return Ok(zones);
            }
        }
    }
}

fn comma_continues_zone_list(next: Option<&str>) -> bool {
    next.is_some_and(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "and"
                | "or"
                | "and/or"
                | "graveyard"
                | "graveyards"
                | "library"
                | "libraries"
                | "exile"
                | "hand"
                | "hands"
                | "battlefield"
        )
    })
}

#[cfg(test)]
#[path = "zone_list_tests.rs"]
mod tests;

/// One registered zone; leaves all list connectors to its caller.
pub(in crate::oracle::canonical) fn recognize_card_zone_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardZoneName> {
    let token = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("a registered card zone"))?;
    let zone = match token.raw.to_ascii_lowercase().as_str() {
        "graveyard" | "graveyards" => CardZoneName::Graveyard,
        "library" | "libraries" => CardZoneName::Library,
        "exile" => CardZoneName::Exile,
        "hand" | "hands" => CardZoneName::Hand,
        "battlefield" => CardZoneName::Battlefield,
        _ => {
            return Err(cursor.fail("graveyard, library, exile or hand (singular or plural)"));
        }
    };
    cursor.require(
        token.token_type == "zoneWord"
            || (zone == CardZoneName::Exile && token.token_type == "actionWord"),
        "a lexically registered zone, not an unresolved word",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(zone)
}
