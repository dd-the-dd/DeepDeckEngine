//! Static life-gain replacements (CR 614.1a): `replaceLifeGain` gives the
//! affected player's life-gain event a new amount computed from the event's
//! quantity. Each program applies once to one event (CR 614.5). Life gain is
//! prepared without a chooser, so programs whose order changes the result
//! are an explicit error rather than an arbitrary order (CR 616.1).
use serde::Deserialize;
use serde_json::Value;

use super::quantity_replacement::QuantityExpression;
use super::{CardInstance, EngineError, GameEngine, value_kind};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LifeGainPattern {
    player: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LifeGainProgram {
    kind: String,
    event: LifeGainPattern,
    quantity: QuantityExpression,
}

fn player_relation_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("eachPlayer") => true,
        Some("controllerOf") => value_kind(&reference["object"]) == Some("self"),
        Some("opponentsOf") => player_relation_supported(&reference["player"]),
        _ => false,
    }
}

fn parse_program(modifier: &Value) -> Option<LifeGainProgram> {
    let program: LifeGainProgram = serde_json::from_value(modifier.clone()).ok()?;
    (program.kind == "replaceLifeGain" && player_relation_supported(&program.event.player))
        .then_some(program)
}

/// Engine coverage of one `replaceLifeGain` modifier.
pub(super) fn life_gain_replacement_modifier_supported(modifier: &Value) -> bool {
    parse_program(modifier).is_some()
}

fn relation_matches(reference: &Value, source: &CardInstance, player_id: &str) -> bool {
    match value_kind(reference) {
        Some("eachPlayer") => true,
        Some("controllerOf") => source.controller == player_id,
        Some("opponentsOf") => !relation_matches(&reference["player"], source, player_id),
        _ => false,
    }
}

/// Final amounts reachable by applying the remaining programs in any order.
fn reachable(
    programs: &[&LifeGainProgram],
    used: &mut [bool],
    amount: i32,
    results: &mut Vec<i32>,
) -> Result<(), EngineError> {
    let mut any = false;
    for index in 0..programs.len() {
        if used[index] || amount <= 0 {
            continue;
        }
        any = true;
        used[index] = true;
        let next = programs[index].quantity.evaluate(amount)?.max(0);
        reachable(programs, used, next, results)?;
        used[index] = false;
    }
    if !any && !results.contains(&amount) {
        results.push(amount);
    }
    Ok(())
}

impl GameEngine {
    /// The replaced life-gain amount and whether any program applied.
    pub(super) fn apply_life_gain_replacements(
        &self,
        player_id: &str,
        amount: i32,
    ) -> Result<(i32, bool), EngineError> {
        let modifiers = self.battlefield_static_modifiers();
        let programs = modifiers
            .iter()
            .filter(|(_, modifier)| value_kind(modifier) == Some("replaceLifeGain"))
            .filter_map(|(source, modifier)| {
                parse_program(modifier)
                    .filter(|program| relation_matches(&program.event.player, source, player_id))
            })
            .collect::<Vec<_>>();
        if programs.is_empty() || amount <= 0 {
            return Ok((amount, false));
        }
        let programs = programs.iter().collect::<Vec<_>>();
        let mut results = Vec::new();
        if programs.len() > 5 {
            return Err(EngineError::new(
                "too many simultaneous life-gain replacements to order without a choice",
            ));
        }
        reachable(
            &programs,
            &mut vec![false; programs.len()],
            amount,
            &mut results,
        )?;
        match results.as_slice() {
            [single] => Ok((*single, true)),
            _ => Err(EngineError::new(
                "life-gain replacements whose order changes the result require the affected player's choice",
            )),
        }
    }
}
