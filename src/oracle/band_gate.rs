//! Threshold bands (level up CR 711, Spacecraft station thresholds, Case `Solved`
//! CR 719.4) gate the abilities printed inside them on a counter count or a
//! designation. Until the band frame lowers that gate, an ability parsed
//! from a band body would apply unconditionally, so it is refused with an
//! explicit `ungated_band_body` diagnostic instead of silently approximated.
//!
//! Class levels are not listed: their bodies already carry the announced
//! `minimumClassLevel` gate.
use super::model::{ParsedOracleAbility, ParserDiagnostic};

/// Which band introducer opened the body an ability belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandIntroducer {
    /// `LEVEL 1-4` / `LEVEL 5+` on its own line: every following line of the
    /// face, up to the next band header, belongs to the band (CR 711.2).
    LevelUp,
    /// `8+ | Flying`: the station threshold prefixes its own line.
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

/// The introducer printed at the start of this ability's own line.
fn line_band_introducer(text: &str) -> Option<BandIntroducer> {
    let text = text.trim_start();
    if let Some((threshold, rest)) = text.split_once('+')
        && is_level_number(threshold)
        && rest.trim_start().starts_with('|')
    {
        return Some(BandIntroducer::Station);
    }
    let solved = text
        .strip_prefix("Solved")
        .map(str::trim_start)
        .is_some_and(|rest| rest.starts_with('\u{2014}') || rest.starts_with('-'));
    solved.then_some(BandIntroducer::CaseSolved)
}

/// The band each ability belongs to, in order. A level band runs from its
/// header to the next header or the end of the same face.
pub(crate) fn band_membership(abilities: &[ParsedOracleAbility]) -> Vec<Option<BandIntroducer>> {
    let mut open_level_band: Option<Option<String>> = None;
    abilities
        .iter()
        .map(|ability| {
            let face = ability.source.face_id.clone();
            if open_level_band
                .as_ref()
                .is_some_and(|band_face| *band_face != face)
            {
                open_level_band = None;
            }
            if level_band_header(&ability.source.text) {
                open_level_band = Some(face);
                // The header itself is the band introducer, not a body.
                return None;
            }
            line_band_introducer(&ability.source.text)
                .or(open_level_band.is_some().then_some(BandIntroducer::LevelUp))
        })
        .collect()
}

/// Refuses every canonical ability inside a band: its rule would run
/// without the band's threshold.
pub(crate) fn reject_ungated_band_bodies(abilities: &mut [ParsedOracleAbility]) {
    let membership = band_membership(abilities);
    for (ability, band) in abilities.iter_mut().zip(membership) {
        let Some(band) = band else {
            continue;
        };
        if ability.status != "canonical" {
            continue;
        }
        let (threshold, rule) = match band {
            BandIntroducer::LevelUp => ("a level-counter threshold", "CR 711.2"),
            BandIntroducer::Station => ("a charge-counter threshold", "Station"),
            BandIntroducer::CaseSolved => ("the solved designation", "CR 719.4"),
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
        });
    }
}

#[cfg(test)]
#[path = "band_gate_tests.rs"]
mod tests;
