use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TurnStep {
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    PostcombatMain,
    EndStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StepPlayerScope {
    AbilityController,
    EachPlayer,
    OpponentsOfAbilityController,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StepEventNode {
    pub step: TurnStep,
    pub player: StepPlayerScope,
}

fn consume_step_suffix(
    cursor: &mut OracleTokenCursor<'_>,
    player: StepPlayerScope,
) -> MachineResult<StepEventNode> {
    cursor.enter("event.step.step");
    let Some(head) = cursor.peek().map(str::to_ascii_lowercase) else {
        return Err(cursor.fail("a supported turn step"));
    };
    let step = match head.as_str() {
        "upkeep" => {
            cursor.consume("upkeep")?;
            TurnStep::Upkeep
        }
        "draw" => {
            cursor.consume_phrase(&["draw", "step"])?;
            TurnStep::Draw
        }
        "end" => {
            cursor.consume_phrase(&["end", "step"])?;
            TurnStep::EndStep
        }
        "first" if player == StepPlayerScope::AbilityController => {
            cursor.consume_phrase(&["first", "main", "phase"])?;
            TurnStep::PrecombatMain
        }
        "postcombat" if player == StepPlayerScope::AbilityController => {
            cursor.consume_phrase(&["postcombat", "main", "phase"])?;
            TurnStep::PostcombatMain
        }
        _ => {
            return Err(cursor
                .fail("upkeep, draw step, end step, first main phase, or postcombat main phase"));
        }
    };
    cursor.enter("event.step.complete");
    Ok(StepEventNode { step, player })
}

/// Recognizes the player-owned turn-step phrase after `At the beginning of`.
/// The temporal prefix and following comma remain parent-owned.
pub(in crate::oracle::canonical) fn recognize_step_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StepEventNode> {
    cursor.enter("event.step.scope");
    let Some(scope) = cursor.peek().map(str::to_ascii_lowercase) else {
        return Err(cursor.fail("`your`, `each`, `the`, or `combat on your turn`"));
    };
    match scope.as_str() {
        "your" => {
            cursor.consume("your")?;
            consume_step_suffix(cursor, StepPlayerScope::AbilityController)
        }
        "each" => {
            cursor.consume("each")?;
            if cursor.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("opponent's") || raw.eq_ignore_ascii_case("opponent’s")
            }) {
                cursor.enter("event.step.opponent-scope");
                cursor.advance_to(cursor.position() + 1)?;
                cursor.enter("event.step.opponent-draw");
                cursor.consume_phrase(&["draw", "step"])?;
                cursor.enter("event.step.complete");
                return Ok(StepEventNode {
                    step: TurnStep::Draw,
                    player: StepPlayerScope::OpponentsOfAbilityController,
                });
            }
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("combat"))
            {
                cursor.consume("combat")?;
                cursor.enter("event.step.complete");
                return Ok(StepEventNode {
                    step: TurnStep::BeginCombat,
                    player: StepPlayerScope::EachPlayer,
                });
            }
            consume_step_suffix(cursor, StepPlayerScope::EachPlayer)
        }
        "the" => {
            cursor.consume("the")?;
            cursor.enter("event.step.unscoped-end-step");
            cursor.consume_phrase(&["end", "step"])?;
            cursor.enter("event.step.complete");
            Ok(StepEventNode {
                step: TurnStep::EndStep,
                player: StepPlayerScope::EachPlayer,
            })
        }
        "combat" => {
            cursor.consume_phrase(&["combat", "on", "your", "turn"])?;
            cursor.enter("event.step.complete");
            Ok(StepEventNode {
                step: TurnStep::BeginCombat,
                player: StepPlayerScope::AbilityController,
            })
        }
        _ => Err(cursor.fail("`your`, `each`, `the`, or `combat on your turn`")),
    }
}
