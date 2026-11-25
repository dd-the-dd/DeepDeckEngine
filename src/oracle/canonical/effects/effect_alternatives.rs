//! Lowering of resolution alternatives (CR 608.2d): `ACTION or ACTION`.
//!
//! The effect sequence reads each alternative as an ordinary action and joins
//! the run with the `Or` connector; the actions are lowered in written order
//! like any other, with the bindings of the whole program. This frame then
//! replaces each maximal run of `or`-joined actions by one `chooseOneEffect`
//! whose options are the lowered actions. The chooser is the player who
//! performs the actions: the controller of an imperative, or the one written
//! performer every option names. Target declarations stay ability-wide
//! (CR 601.2c): a target an unchosen option names is still declared.
use super::*;

/// One run of alternatives: the first action and the action after the last.
struct AlternativeRun {
    first_effect: usize,
    end_effect: usize,
}

fn alternative_runs(sequence: &EffectSequenceNode) -> Vec<AlternativeRun> {
    let mut runs = Vec::new();
    let mut index = 0;
    while index < sequence.connectors.len() {
        if sequence.connectors[index] != EffectConnector::Or {
            index += 1;
            continue;
        }
        let first_effect = index;
        while index < sequence.connectors.len() && sequence.connectors[index] == EffectConnector::Or
        {
            index += 1;
        }
        runs.push(AlternativeRun {
            first_effect,
            end_effect: index + 1,
        });
    }
    runs
}

/// The one player who chooses among lowered options: the controller when no
/// primitive names a performer, else the single performer every primitive
/// of every option names. A mixture has no single chooser.
fn alternative_chooser(options: &[Vec<Value>]) -> Option<Value> {
    let performers = options
        .iter()
        .flatten()
        .map(|effect| effect.get("performer").cloned())
        .collect::<Vec<_>>();
    match performers.first()? {
        None => performers.iter().all(Option::is_none).then(controller),
        Some(performer) => performers
            .iter()
            .all(|other| other.as_ref() == Some(performer))
            .then(|| performer.clone()),
    }
}

/// Whether a lowered primitive distributes its body over several players or
/// objects (`each opponent ...`): its options are no longer one player's.
fn distributes_performers(effect: &Value) -> bool {
    matches!(
        effect.get("kind").and_then(Value::as_str),
        Some("forEachPlayer" | "forEachObject")
    ) && effect["effects"]
        .as_array()
        .is_some_and(|body| body.iter().any(|inner| inner.get("performer").is_some()))
}

/// Replaces each run of `or`-joined actions by one resolution choice and
/// keeps `action_starts` aligned for the frames lowered after it.
pub(super) fn apply_alternative_frames(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    action_starts: &mut [(usize, usize)],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let runs = alternative_runs(sequence);
    // Later runs first, so earlier positions stay valid while splicing.
    for run in runs.iter().rev() {
        cursor.enter("lower.alternatives.options");
        let from = action_starts[run.first_effect].0;
        let to = action_starts[run.end_effect].0;
        let mut options = Vec::with_capacity(run.end_effect - run.first_effect);
        for effect_index in run.first_effect..run.end_effect {
            let start = action_starts[effect_index].0;
            let end = action_starts[effect_index + 1].0;
            cursor.require(
                start < end,
                "an alternative that lowers to at least one executable step",
            )?;
            options.push(effects[start..end].to_vec());
        }
        cursor.enter("lower.alternatives.chooser");
        if options.iter().flatten().any(distributes_performers) {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                "alternatives one player chooses between (each player's own choice is not represented)",
            )));
        }
        let chooser = alternative_chooser(&options).ok_or_else(|| {
            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one player performing every alternative, who chooses among them"),
            )
        })?;
        let options = options
            .into_iter()
            .enumerate()
            .map(|(index, effects)| {
                json!({
                    "label": format!("alternative {}", index + 1),
                    "effects": effects,
                })
            })
            .collect::<Vec<_>>();
        effects.splice(
            from..to,
            [json!({
                "kind": "chooseOneEffect",
                "player": chooser,
                "options": options,
            })],
        );
        for start in action_starts.iter_mut() {
            if start.0 >= to {
                start.0 = start.0 - (to - from) + 1;
            } else if start.0 > from {
                start.0 = from;
            }
        }
    }
    Ok(())
}
