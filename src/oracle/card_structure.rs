//! Card topology is validated before any Oracle grammar is read.
//!
//! A successful plan describes printed parts and their relationship. It is
//! neither an Oracle coverage claim nor proof of executable gameplay rules.

use serde::Serialize;

use super::model::{OracleCardParseRequest, ParserDiagnostic};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardStructureKind {
    Single,
    Transform,
    ModalDoubleFaced,
    Flip,
    Split,
    Rooms,
    Adventure,
    Omen,
    Preparation,
    DoubleFacedToken,
    ArtSeries,
    MeldComponent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardPartRole {
    Primary,
    Front,
    Back,
    SplitHalf,
    RoomDoor,
    AdventureSpell,
    OmenSpell,
    PreparedSpell,
    PrintedSide,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardStructurePart {
    /// None denotes the request's top-level text; otherwise index into faces.
    pub face_index: Option<usize>,
    pub face_id: Option<String>,
    pub role: CardPartRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardStructurePlan {
    pub kind: CardStructureKind,
    pub layout: Option<String>,
    pub parts: Vec<CardStructurePart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardStructureFailure {
    pub code: &'static str,
    pub state: &'static str,
    pub message: String,
    pub expected: String,
    pub found: String,
}

impl CardStructureFailure {
    pub(crate) fn into_diagnostic(self) -> ParserDiagnostic {
        ParserDiagnostic {
            code: self.code.to_string(),
            message: self.message,
            severity: "unsupported".to_string(),
            automaton: Some("card-face-factory".to_string()),
            state: Some(self.state.to_string()),
            token_index: None,
            span_start: None,
            span_end: None,
            expected: Some(self.expected),
            found: Some(self.found),
            trace: Vec::new(),
            owner: None,
        }
    }
}

fn failure(
    request: &OracleCardParseRequest,
    missing: bool,
    state: &'static str,
    expected: &str,
) -> CardStructureFailure {
    let code = if missing {
        "card_face_factory_missing"
    } else {
        "card_face_structure_invalid"
    };
    let found = format!(
        "layout={:?}; faceCount={}; faceTypes={:?}",
        request.layout,
        request.faces.len(),
        request
            .faces
            .iter()
            .map(|face| face.type_line.as_str())
            .collect::<Vec<_>>()
    );
    CardStructureFailure {
        code,
        state,
        message: format!(
            "{} for card {:?}: expected {expected}; {found}. {}",
            if missing {
                "A card-face factory is missing"
            } else {
                "Invalid card-face structure"
            },
            request.card_name,
            if missing {
                "Implement or select the shared structure factory before extending Oracle grammar"
            } else {
                "Check the complete card metadata before changing Oracle grammar"
            }
        ),
        expected: expected.to_string(),
        found,
    }
}

fn has_word(type_line: &str, expected: &str) -> bool {
    type_line
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case(expected))
}

fn is_spell(type_line: &str) -> bool {
    has_word(type_line, "Instant") || has_word(type_line, "Sorcery")
}

fn has_spell_subtype(type_line: &str, subtype: &str) -> bool {
    // A type header is metadata, not an Oracle effect sentence. Honor the
    // actual subtype boundary instead of matching the card's name or text.
    type_line
        .split_once('—')
        .or_else(|| type_line.split_once(" - "))
        .is_some_and(|(_, subtypes)| has_word(subtypes, subtype))
}

fn require_count(
    request: &OracleCardParseRequest,
    valid: bool,
    expected: &str,
) -> Result<(), CardStructureFailure> {
    if valid {
        Ok(())
    } else {
        Err(failure(request, false, "structure.parts", expected))
    }
}

/// Select a shared structural factory from layout and typed part metadata.
/// Unknown or ambiguous structures never fall back to a flattened text.
pub fn plan_card_structure(
    request: &OracleCardParseRequest,
) -> Result<CardStructurePlan, CardStructureFailure> {
    let count = request.faces.len();
    let kind = match request.layout.as_deref() {
        None if count <= 1 => CardStructureKind::Single,
        None => {
            return Err(failure(
                request,
                true,
                "factory.layout-required",
                "an explicit layout for a card with multiple printed parts",
            ));
        }
        Some(
            "normal" | "token" | "class" | "saga" | "prototype" | "leveler" | "planar" | "scheme"
            | "vanguard" | "emblem" | "host" | "augment" | "mutate" | "case" | "front_card",
        ) => {
            require_count(request, count <= 1, "zero or one explicit part")?;
            CardStructureKind::Single
        }
        Some("meld") => {
            // A meld partner/result is a different card. This request only
            // describes the local component; no partner text is invented.
            require_count(request, count <= 1, "one local meld component")?;
            CardStructureKind::MeldComponent
        }
        Some("transform" | "modal_dfc" | "flip" | "double_faced_token" | "art_series") => {
            require_count(request, count == 2, "exactly two explicit printed parts")?;
            match request.layout.as_deref() {
                Some("transform") => CardStructureKind::Transform,
                Some("modal_dfc") => CardStructureKind::ModalDoubleFaced,
                Some("flip") => CardStructureKind::Flip,
                Some("double_faced_token") => CardStructureKind::DoubleFacedToken,
                _ => CardStructureKind::ArtSeries,
            }
        }
        Some("split") => {
            require_count(request, count >= 2, "at least two explicit split parts")?;
            let rooms = request
                .faces
                .iter()
                .filter(|face| has_spell_subtype(&face.type_line, "Room"))
                .count();
            if rooms == count {
                require_count(request, count == 2, "exactly two Room doors")?;
                CardStructureKind::Rooms
            } else {
                // Split is an ordered-part envelope. Only the complete Room
                // shape receives door semantics; a partially matching shape
                // remains Split, never a partially assembled Room.
                CardStructureKind::Split
            }
        }
        Some("adventure" | "omen" | "prepare") => {
            require_count(
                request,
                count == 2,
                "a primary part and one associated spell",
            )?;
            let primary = &request.faces[0];
            let spell = &request.faces[1];
            require_count(
                request,
                !primary.type_line.trim().is_empty() && is_spell(&spell.type_line),
                "a primary part followed by an instant or sorcery alternative",
            )?;
            let adventure = has_spell_subtype(&spell.type_line, "Adventure");
            let omen = has_spell_subtype(&spell.type_line, "Omen");
            match (request.layout.as_deref(), adventure, omen) {
                (Some("prepare"), false, false) if !is_spell(&primary.type_line) => {
                    CardStructureKind::Preparation
                }
                (Some("adventure"), true, false) => CardStructureKind::Adventure,
                (Some("adventure" | "omen"), false, true) => CardStructureKind::Omen,
                (Some("adventure"), false, false) => {
                    return Err(failure(
                        request,
                        true,
                        "factory.alternative-spell-profile",
                        "a registered alternative-spell profile, identified by its spell subtype",
                    ));
                }
                _ => {
                    return Err(failure(
                        request,
                        false,
                        "structure.alternative-spell-subtype",
                        "an Adventure or Omen subtype consistent with its layout, or an untagged prepare spell",
                    ));
                }
            }
        }
        Some(_) => {
            return Err(failure(
                request,
                true,
                "factory.layout",
                "a registered card-face factory for this layout",
            ));
        }
    };

    for (index, face) in request.faces.iter().enumerate() {
        if face.id.trim().is_empty()
            || request.faces[..index]
                .iter()
                .any(|previous| previous.id == face.id)
        {
            return Err(failure(
                request,
                false,
                "structure.part-identity",
                "a nonempty, unique identity for each printed part",
            ));
        }
    }
    let role = |index| match kind {
        CardStructureKind::Transform
        | CardStructureKind::ModalDoubleFaced
        | CardStructureKind::Flip => {
            if index == 0 {
                CardPartRole::Front
            } else {
                CardPartRole::Back
            }
        }
        CardStructureKind::Split => CardPartRole::SplitHalf,
        CardStructureKind::Rooms => CardPartRole::RoomDoor,
        CardStructureKind::Adventure if index == 1 => CardPartRole::AdventureSpell,
        CardStructureKind::Omen if index == 1 => CardPartRole::OmenSpell,
        CardStructureKind::Preparation if index == 1 => CardPartRole::PreparedSpell,
        CardStructureKind::DoubleFacedToken | CardStructureKind::ArtSeries => {
            CardPartRole::PrintedSide
        }
        _ => CardPartRole::Primary,
    };
    let parts = if count == 0 {
        vec![CardStructurePart {
            face_index: None,
            face_id: None,
            role: CardPartRole::Primary,
        }]
    } else {
        request
            .faces
            .iter()
            .enumerate()
            .map(|(index, face)| CardStructurePart {
                face_index: Some(index),
                face_id: Some(face.id.clone()),
                role: role(index),
            })
            .collect()
    };
    Ok(CardStructurePlan {
        kind,
        layout: request.layout.clone(),
        parts,
    })
}

#[cfg(test)]
#[path = "card_structure_tests.rs"]
mod tests;
