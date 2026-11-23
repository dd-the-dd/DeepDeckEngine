//! Threshold bands are typed containers of the logical-ability boundary pass
//! (level symbols CR 711.2, station symbols CR 722.2, Case `Solved` CR
//! 702.169). The band introducer is recognized structurally before any body
//! is tokenized; every ability printed inside the band is then compiled by
//! the ordinary per-ability parser (the shared body slot) and carries the
//! band's typed gate. A body is never accepted without its gate:
//!
//! ```text
//! LevelBand    := "LEVEL" N "-" M | "LEVEL" N "+"     (own line, CR 711.2a-b)
//!                 { Ability | P/T }                   (until the next band or face end)
//! StationBand  := N "+" "|" Ability { Ability }       (Spacecraft or Station face)
//! SolvedBand   := "Solved" "—" Ability                (its own line, CR 702.169)
//! ```
//!
//! Class levels are not bands here: their bodies already carry the announced
//! `minimumClassLevel` gate.
use serde_json::{Value, json};

use super::model::{AbilitySource, ParserDiagnostic};

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

/// The typed threshold of a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandGate {
    /// At least `minimum` and, when bounded, at most `maximum` counters of
    /// one kind on the permanent (CR 711.2a-b level, 722.2a charge).
    CounterRange {
        counter: &'static str,
        minimum: u32,
        maximum: Option<u32>,
    },
    /// The Case is solved (CR 702.169b-d).
    Solved,
}

impl BandGate {
    /// The rule metadata the runtime reads through its one band-gate reader.
    pub(crate) fn lower(self) -> Value {
        match self {
            Self::CounterRange {
                counter,
                minimum,
                maximum,
            } => {
                let mut gate = json!({
                    "kind": "counterRange",
                    "counter": counter,
                    "minimum": minimum,
                });
                if let Some(maximum) = maximum {
                    gate["maximum"] = json!(maximum);
                }
                gate
            }
            Self::Solved => json!({ "kind": "designation", "designation": "solved" }),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::CounterRange {
                counter: LEVEL_COUNTER,
                ..
            } => "a level-counter threshold (CR 711.2)",
            Self::CounterRange { .. } => "a charge-counter threshold (CR 722.2)",
            Self::Solved => "the solved designation (CR 702.169)",
        }
    }
}

const LEVEL_COUNTER: &str = "level";
const CHARGE_COUNTER: &str = "charge";

/// One band body line: its introducer, gate, and the byte offset at which
/// the ability text starts (after `N+ |` or `Solved —`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BandBody {
    pub introducer: BandIntroducer,
    pub gate: BandGate,
    pub body_start: usize,
}

/// The role of one logical line with respect to bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BandSlot {
    Outside,
    /// A level header line: the band's introducer, not an ability.
    Header,
    Body(BandBody),
}

fn parse_count(raw: &str) -> Option<u32> {
    (!raw.is_empty() && raw.chars().all(|character| character.is_ascii_digit()))
        .then(|| raw.parse().ok())
        .flatten()
}

/// `LEVEL N-M` or `LEVEL N+`, printed in capitals as a whole line.
fn level_band_header(text: &str) -> Option<BandGate> {
    let range = text.trim().strip_prefix("LEVEL ")?;
    let (minimum, maximum) = if let Some(minimum) = range.strip_suffix('+') {
        (parse_count(minimum)?, None)
    } else {
        let (low, high) = range.split_once('-')?;
        let (low, high) = (parse_count(low)?, parse_count(high)?);
        if high < low {
            return None;
        }
        (low, Some(high))
    };
    Some(BandGate::CounterRange {
        counter: LEVEL_COUNTER,
        minimum,
        maximum,
    })
}

/// `8+ | Flying`: a station threshold opening a band; returns the threshold
/// and the byte offset after the `|` separator.
fn station_band_header(text: &str) -> Option<(u32, usize)> {
    let leading = text.len() - text.trim_start().len();
    let (threshold, rest) = text[leading..].split_once('+')?;
    let minimum = parse_count(threshold)?;
    let after_plus = leading + threshold.len() + 1;
    let separator = rest.trim_start();
    if !separator.starts_with('|') {
        return None;
    }
    let separator_offset = after_plus + (rest.len() - separator.len());
    Some((minimum, separator_offset + 1))
}

/// `Solved — ...`: returns the byte offset after the dash.
fn case_solved_line(text: &str) -> Option<usize> {
    let leading = text.len() - text.trim_start().len();
    let rest = text[leading..].strip_prefix("Solved")?;
    let dash = rest.trim_start();
    let dash_width = if dash.starts_with('\u{2014}') {
        '\u{2014}'.len_utf8()
    } else if dash.starts_with('-') {
        1
    } else {
        return None;
    };
    Some(text.len() - dash.len() + dash_width)
}

/// A face whose `N+ |` lines are station thresholds: a Spacecraft, or a face
/// with a Station keyword line. Elsewhere `15+ | Scry` is a die-roll result
/// row, not a band.
fn station_face(type_line: &str, sources: &[AbilitySource]) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("Spacecraft"))
        || sources.iter().any(|source| {
            source
                .text
                .trim_start()
                .strip_prefix("Station")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(']))
        })
}

/// The band slot of each logical line of one face, in order. A level or
/// station band runs from its header to the next band header or the end of
/// the face; a `Solved` line is its own band.
pub(crate) fn plan_band_slots(face_type_line: &str, sources: &[AbilitySource]) -> Vec<BandSlot> {
    let station = station_face(face_type_line, sources);
    let mut open: Option<(BandIntroducer, BandGate)> = None;
    sources
        .iter()
        .map(|source| {
            let text = source.text.as_str();
            if let Some(gate) = level_band_header(text) {
                open = Some((BandIntroducer::LevelUp, gate));
                return BandSlot::Header;
            }
            if station && let Some((minimum, body_start)) = station_band_header(text) {
                let gate = BandGate::CounterRange {
                    counter: CHARGE_COUNTER,
                    minimum,
                    maximum: None,
                };
                open = Some((BandIntroducer::Station, gate));
                return BandSlot::Body(BandBody {
                    introducer: BandIntroducer::Station,
                    gate,
                    body_start,
                });
            }
            if let Some(body_start) = case_solved_line(text) {
                return BandSlot::Body(BandBody {
                    introducer: BandIntroducer::CaseSolved,
                    gate: BandGate::Solved,
                    body_start,
                });
            }
            match open {
                Some((introducer, gate)) => BandSlot::Body(BandBody {
                    introducer,
                    gate,
                    body_start: 0,
                }),
                None => BandSlot::Outside,
            }
        })
        .collect()
}

/// The ability text printed inside a band, without its introducer. Line
/// numbers are kept; only the introducer prefix is removed.
pub(crate) fn band_body_source(source: AbilitySource, body: &BandBody) -> AbilitySource {
    if body.body_start == 0 {
        return source;
    }
    AbilitySource {
        text: source.text[body.body_start..].trim().to_string(),
        ..source
    }
}

/// Attaches the band's gate to a rule compiled from its body.
pub(crate) fn apply_band_gate(rule: &mut Value, body: &BandBody) -> Result<(), ParserDiagnostic> {
    // The runtime consults a source's band only for battlefield statics,
    // keywords, triggers and battlefield activations; a gated mana ability
    // (or any other rule kind) would run ungated, so it is an engine gap.
    let reads_gate = match rule["kind"].as_str() {
        Some("staticAbility" | "keywordAbility" | "keywordAbilityGroup" | "triggeredAbility") => {
            true
        }
        Some("activatedAbility") => rule
            .get("activationZone")
            .is_none_or(|zone| zone.as_str() == Some("battlefield")),
        _ => false,
    };
    if !reads_gate {
        return Err(band_diagnostic(
            "oracle_machine_failure",
            format!(
                "This {} is gated by {}, but the runtime does not gate that rule kind.",
                rule["kind"].as_str().unwrap_or("rule"),
                body.gate.description()
            ),
            "engine.gap.band-gated-rule",
            "a rule kind whose band gate the runtime consults",
        ));
    }
    rule["bandGate"] = body.gate.lower();
    Ok(())
}

/// The outcome of reading a band body line as a printed power/toughness box.
pub(crate) enum BandPowerToughness {
    /// The line is an ordinary ability.
    NotPowerToughness,
    /// CR 711.2a-b: the leveler has that base power and toughness while the
    /// band is open, as one gated static ability.
    Rule(Value),
    /// A P/T box whose band meaning is not lowered (a station P/T also makes
    /// the permanent a creature, CR 722.2b).
    Unsupported(ParserDiagnostic),
}

fn power_toughness_box(text: &str) -> Option<(u32, u32)> {
    let (power, toughness) = text.trim().split_once('/')?;
    Some((parse_count(power)?, parse_count(toughness)?))
}

pub(crate) fn band_power_toughness(text: &str, body: &BandBody) -> BandPowerToughness {
    let Some((power, toughness)) = power_toughness_box(text) else {
        return BandPowerToughness::NotPowerToughness;
    };
    match body.introducer {
        BandIntroducer::LevelUp => {
            let mut rule = json!({
                "kind": "staticAbility",
                "source": { "kind": "self" },
                "activeWhile": {
                    "kind": "inZone",
                    "object": { "kind": "self" },
                    "zone": { "kind": "battlefield" },
                },
                "modifiers": [{
                    "kind": "setBasePowerToughness",
                    "objects": { "kind": "self" },
                    "power": { "kind": "integer", "value": power },
                    "toughness": { "kind": "integer", "value": toughness },
                }],
            });
            apply_band_gate(&mut rule, body)
                .expect("a printed P/T box is a static ability its band gates");
            BandPowerToughness::Rule(rule)
        }
        BandIntroducer::Station | BandIntroducer::CaseSolved => {
            BandPowerToughness::Unsupported(band_diagnostic(
                "band_power_toughness_unsupported",
                format!(
                    "This power/toughness box is printed inside a band gated by {}; it also makes the permanent a creature (CR 722.2b), which is not lowered yet.",
                    body.gate.description()
                ),
                "card-structure.band.power-toughness",
                "a band power/toughness box with a lowered type change",
            ))
        }
    }
}

/// A band whose introducer carries no ability text (`8+ |` alone).
pub(crate) fn empty_band_body_diagnostic(body: &BandBody) -> ParserDiagnostic {
    band_diagnostic(
        "empty_band_body",
        format!(
            "This band introducer is gated by {} but carries no ability text.",
            body.gate.description()
        ),
        "card-structure.band.empty-body",
        "an ability after the band introducer",
    )
}

fn band_diagnostic(code: &str, message: String, state: &str, expected: &str) -> ParserDiagnostic {
    ParserDiagnostic {
        code: code.to_string(),
        message,
        severity: "unsupported".to_string(),
        automaton: Some("card-face-factory".to_string()),
        state: Some(state.to_string()),
        token_index: None,
        span_start: None,
        span_end: None,
        expected: Some(expected.to_string()),
        found: None,
        trace: Vec::new(),
        owner: None,
    }
}

#[cfg(test)]
#[path = "band_gate_tests.rs"]
mod tests;
