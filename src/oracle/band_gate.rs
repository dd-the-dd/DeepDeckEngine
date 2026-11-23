//! Threshold bands (level up CR 711.2, station symbols CR 722.2, Case `Solved`
//! CR 720.3c) gate the abilities printed inside them on a counter count or a
//! designation. Until the band frame lowers that gate, an ability parsed
//! from a band body would apply unconditionally, so it is refused with an
//! explicit `ungated_band_body` diagnostic instead of silently approximated.
//!
//! Class levels are not listed: their bodies already carry the announced
//! `minimumClassLevel` gate.
use super::model::{OracleCardParseRequest, ParsedOracleAbility, ParserDiagnostic};

/// Which band introducer opened the body an ability belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandIntroducer {
    /// `LEVEL 1-4` / `LEVEL 5+` on its own line: every following line of the
    /// face, up to the next band header, belongs to the band (CR 711.2).
    LevelUp,
    /// `8+ | Flying`: a station threshold on a Spacecraft face; the band
    /// runs to the next band header or the end of the face.
    Station,
    /// `Solved — ...`: the Case designation prefixes its own line.
    CaseSolved,
}

fn is_level_number(raw: &str) -> bool {
    !raw.is_empty() && raw.chars().all(|character| character.is_ascii_digit())
}

/// `LEVEL N-M` or `LEVEL N+`, printed in capitals as a whole line.
fn level_band_header(text: &str) -> bool {
    let Some(range) = text.trim().strip_prefix("LEVEL ") else {
        return false;
    };
    if let Some(minimum) = range.strip_suffix('+') {
        return is_level_number(minimum);
    }
    range
        .split_once('-')
        .is_some_and(|(low, high)| is_level_number(low) && is_level_number(high))
}

/// `8+ | Flying`: a station threshold opening a band on its own line.
fn station_band_header(text: &str) -> bool {
    text.trim_start()
        .split_once('+')
        .is_some_and(|(threshold, rest)| {
            is_level_number(threshold) && rest.trim_start().starts_with('|')
        })
}

/// `Solved — ...`: the Case designation gates only its own line.
fn case_solved_line(text: &str) -> bool {
    text.trim_start()
        .strip_prefix("Solved")
        .map(str::trim_start)
        .is_some_and(|rest| rest.starts_with('\u{2014}') || rest.starts_with('-'))
}

/// A face whose `N+ |` lines are station thresholds: a Spacecraft, or a face
/// with a Station keyword line. Elsewhere `15+ | Scry` is a die-roll result
/// row, not a band.
fn station_face(type_line: &str, face_abilities: &[&ParsedOracleAbility]) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("Spacecraft"))
        || face_abilities.iter().any(|ability| {
            ability
                .source
                .text
                .trim_start()
                .strip_prefix("Station")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(']))
        })
}

/// The band each ability belongs to, in order. A level or station band runs
/// from its header to the next band header or the end of the same face; a
/// `Solved` line is its own band. `face_type_line` names each face's type
/// line by its face id.
pub(crate) fn band_membership(
    abilities: &[ParsedOracleAbility],
    face_type_line: impl Fn(Option<&str>) -> String,
) -> Vec<Option<BandIntroducer>> {
    let mut open: Option<(Option<String>, BandIntroducer)> = None;
    abilities
        .iter()
        .map(|ability| {
            let face = ability.source.face_id.clone();
            if open
                .as_ref()
                .is_some_and(|(band_face, _)| *band_face != face)
            {
                open = None;
            }
            let text = &ability.source.text;
            if level_band_header(text) {
                open = Some((face, BandIntroducer::LevelUp));
                // The header itself is the band introducer, not a body.
                return None;
            }
            if station_band_header(text) {
                let face_abilities = abilities
                    .iter()
                    .filter(|other| other.source.face_id == face)
                    .collect::<Vec<_>>();
                if station_face(&face_type_line(face.as_deref()), &face_abilities) {
                    // The header line carries the band's first ability.
                    open = Some((face, BandIntroducer::Station));
                    return Some(BandIntroducer::Station);
                }
            }
            if case_solved_line(text) {
                return Some(BandIntroducer::CaseSolved);
            }
            open.as_ref().map(|(_, band)| *band)
        })
        .collect()
}

/// Refuses every canonical ability inside a band: its rule would run
/// without the band's threshold.
pub(crate) fn reject_ungated_band_bodies(
    request: &OracleCardParseRequest,
    abilities: &mut [ParsedOracleAbility],
) {
    let membership = band_membership(abilities, |face_id| match face_id {
        Some(id) => request
            .faces
            .iter()
            .find(|face| face.id == id)
            .map_or_else(|| request.type_line.clone(), |face| face.type_line.clone()),
        None => request.type_line.clone(),
    });
    for (ability, band) in abilities.iter_mut().zip(membership) {
        let Some(band) = band else {
            continue;
        };
        if ability.status != "canonical" {
            continue;
        }
        let (threshold, rule) = match band {
            BandIntroducer::LevelUp => ("a level-counter threshold", "CR 711.2"),
            BandIntroducer::Station => ("a charge-counter threshold", "CR 722.2"),
            BandIntroducer::CaseSolved => ("the solved designation", "CR 720.3c"),
        };
        ability.status = "unsupported".to_string();
        ability.rule = None;
        ability.diagnostics.push(ParserDiagnostic {
            code: "ungated_band_body".to_string(),
            message: format!(
                "This ability is printed inside a band gated by {threshold} ({rule}); the band gate is not lowered yet, so the ability would apply unconditionally."
            ),
            severity: "unsupported".to_string(),
            automaton: Some("card-face-factory".to_string()),
            state: Some("card-structure.band.ungated-body".to_string()),
            token_index: None,
            span_start: None,
            span_end: None,
            expected: Some(format!("a band frame gating this ability on {threshold}")),
            found: None,
            trace: Vec::new(),
            owner: None,
        });
    }
}

#[cfg(test)]
#[path = "band_gate_tests.rs"]
mod tests;
