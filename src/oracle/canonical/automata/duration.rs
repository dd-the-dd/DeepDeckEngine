use super::super::{MachineResult, OracleTokenCursor};
use super::EffectDurationNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectDurationState {
    Boundary,
    PointOrPossessor,
    Relation,
    Next,
    Unit,
    Complete,
}

/// Recognizes the canonical duration that expires during the cleanup step of
/// the current turn. Effect-specific parents retain ownership of the modifier
/// or continuous effect to which this duration applies.
pub(in crate::oracle::canonical) fn recognize_effect_duration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectDurationNode> {
    let mut state = EffectDurationState::Boundary;
    let mut duration = None;
    loop {
        state = match state {
            EffectDurationState::Boundary => {
                cursor.enter("duration.until-end-of-turn.boundary");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("until") => {
                        cursor.consume("until")?;
                        EffectDurationState::PointOrPossessor
                    }
                    Some("this") => {
                        cursor.consume("this")?;
                        duration = Some(EffectDurationNode::UntilEndOfCurrentTurn);
                        EffectDurationState::Unit
                    }
                    _ => {
                        return Err(cursor.fail("`this turn` or a duration beginning with `until`"));
                    }
                }
            }
            EffectDurationState::PointOrPossessor => {
                cursor.enter("duration.point-or-possessor");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("end") => {
                        cursor.consume("end")?;
                        duration = Some(EffectDurationNode::UntilEndOfCurrentTurn);
                        EffectDurationState::Relation
                    }
                    Some("your") => {
                        cursor.consume("your")?;
                        duration = Some(EffectDurationNode::UntilNextTurn {
                            player: super::LocalPlayerReference::Controller,
                        });
                        EffectDurationState::Next
                    }
                    _ => return Err(cursor.fail("`end` or the controller possessive `your`")),
                }
            }
            EffectDurationState::Relation => {
                cursor.enter("duration.until-end-of-turn.relation");
                cursor.consume("of")?;
                EffectDurationState::Unit
            }
            EffectDurationState::Next => {
                cursor.enter("duration.until-next-turn.next");
                cursor.consume("next")?;
                EffectDurationState::Unit
            }
            EffectDurationState::Unit => {
                cursor.enter("duration.until-end-of-turn.unit");
                cursor.consume("turn")?;
                EffectDurationState::Complete
            }
            EffectDurationState::Complete => {
                cursor.enter("duration.complete");
                return cursor.require_value(duration, "a populated effect duration");
            }
        };
    }
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;
