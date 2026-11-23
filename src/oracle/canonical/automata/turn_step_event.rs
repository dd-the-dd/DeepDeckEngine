//! Turn-step events after `At the beginning of` (CR 503.1, 504, 505, 507,
//! 513). The event composes two independent concepts:
//!
//! ```text
//! StepEvent  := TurnOwner StepName
//!             | "each" StepName                        -- any player's step
//!             | "each" "of" TurnOwner PluralStepName
//!             | "the" StepName ["of" AttachedController]
//!             | "each" "combat" | "combat" "on" TurnOwner "turn"
//! TurnOwner  := "your" | "your opponents'" | "each player's" | "each opponent's"
//! StepName   := "upkeep" | "draw step" | "end step"
//!             | ("first" | "precombat") "main phase"
//!             | "postcombat" "main phase"
//!             | "second" "main phase"               -- main phase ordinal 2
//! ```
//!
//! The owner names whose turn the step belongs to; the step name is shared by
//! every owner. The temporal prefix and following comma remain parent-owned.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TurnStep {
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    PostcombatMain,
    /// `second main phase`: the main phase with ordinal 2 in its turn
    /// (CR 505.1a), one particular postcombat main phase.
    SecondMain,
    EndStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StepPlayerScope {
    AbilityController,
    EachPlayer,
    OpponentsOfAbilityController,
    /// The controller of the permanent the source is attached to
    /// (`the upkeep of enchanted creature's controller`, CR 303.4), with
    /// the nominal the text wrote for that permanent.
    AttachedPermanentController(super::TargetNominal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StepEventNode {
    pub step: TurnStep,
    pub player: StepPlayerScope,
}

fn is_possessive(raw: &str, noun: &str) -> bool {
    raw.eq_ignore_ascii_case(&format!("{noun}'s")) || raw.eq_ignore_ascii_case(&format!("{noun}’s"))
}

fn is_plural_possessive(raw: &str, noun: &str) -> bool {
    raw.eq_ignore_ascii_case(&format!("{noun}'")) || raw.eq_ignore_ascii_case(&format!("{noun}’"))
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.advance_to(cursor.position() + 1)
}

/// Whose turn: `your`, `your opponents'`, `each player's`, `each opponent's`.
fn recognize_turn_owner(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<StepPlayerScope> {
    cursor.enter("event.step.turn-owner");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn owner"))?;
    match head.as_str() {
        "your" => {
            cursor.consume("your")?;
            if cursor
                .peek()
                .is_some_and(|raw| is_plural_possessive(raw, "opponents"))
            {
                consume_current(cursor)?;
                return Ok(StepPlayerScope::OpponentsOfAbilityController);
            }
            Ok(StepPlayerScope::AbilityController)
        }
        "each" => {
            cursor.consume("each")?;
            cursor.enter("event.step.turn-owner.player");
            let owner = match cursor.peek() {
                Some(raw) if is_possessive(raw, "player") => StepPlayerScope::EachPlayer,
                Some(raw) if is_possessive(raw, "opponent") => {
                    StepPlayerScope::OpponentsOfAbilityController
                }
                _ => return Err(cursor.fail("`player's` or `opponent's`")),
            };
            consume_current(cursor)?;
            Ok(owner)
        }
        _ => Err(cursor.fail("`your`, `your opponents'`, `each player's` or `each opponent's`")),
    }
}

/// One step name; `plural` reads the form after `each of`.
fn recognize_step_name(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<TurnStep> {
    cursor.enter("event.step.step");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn step name"))?;
    let (step, words): (TurnStep, &[&str]) = match (head.as_str(), plural) {
        ("upkeep", false) => (TurnStep::Upkeep, &["upkeep"]),
        ("upkeeps", true) => (TurnStep::Upkeep, &["upkeeps"]),
        ("draw", false) => (TurnStep::Draw, &["draw", "step"]),
        ("draw", true) => (TurnStep::Draw, &["draw", "steps"]),
        ("end", false) => (TurnStep::EndStep, &["end", "step"]),
        ("end", true) => (TurnStep::EndStep, &["end", "steps"]),
        ("first" | "precombat", false) => (TurnStep::PrecombatMain, &["main", "phase"]),
        ("first" | "precombat", true) => (TurnStep::PrecombatMain, &["main", "phases"]),
        ("postcombat", false) => (TurnStep::PostcombatMain, &["main", "phase"]),
        ("postcombat", true) => (TurnStep::PostcombatMain, &["main", "phases"]),
        // CR 505.1a: every main phase after the first is a postcombat main
        // phase; `second main phase` names the one with ordinal 2, a typed
        // ordinal rather than any postcombat main phase.
        ("second", false) => (TurnStep::SecondMain, &["main", "phase"]),
        ("second", true) => (TurnStep::SecondMain, &["main", "phases"]),
        _ => {
            return Err(cursor.fail(
                "upkeep, draw step, end step, first/precombat, second or postcombat main phase",
            ));
        }
    };
    if matches!(
        step,
        TurnStep::PrecombatMain | TurnStep::PostcombatMain | TurnStep::SecondMain
    ) {
        consume_current(cursor)?;
    }
    cursor.consume_phrase(words)?;
    Ok(step)
}

/// `of enchanted PERMANENT's controller` after `the STEP`: the turn owner is
/// the controller of the object the source enchants.
fn recognize_attached_controller(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StepPlayerScope> {
    cursor.enter("event.step.attached-controller");
    cursor.consume_phrase(&["of", "enchanted"])?;
    cursor.enter("event.step.attached-controller.object");
    let noun = ["creature", "artifact", "enchantment", "land", "permanent"]
        .into_iter()
        .find(|noun| cursor.peek().is_some_and(|raw| is_possessive(raw, noun)))
        .ok_or_else(|| cursor.fail("an enchanted permanent's possessive"))?;
    let nominal =
        match crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(noun) {
            Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::CardType(
                value,
            )) => super::TargetNominal::CardType(value),
            Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::ObjectDomain(
                value,
            )) => super::TargetNominal::Domain(value),
            _ => return Err(cursor.fail("a registered enchanted permanent nominal")),
        };
    consume_current(cursor)?;
    cursor.enter("event.step.attached-controller.relation");
    cursor.consume("controller")?;
    Ok(StepPlayerScope::AttachedPermanentController(nominal))
}

/// Recognizes the player-owned turn-step phrase after `At the beginning of`.
pub(in crate::oracle::canonical) fn recognize_step_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StepEventNode> {
    cursor.enter("event.step.scope");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn-step owner"))?;
    let node = match head.as_str() {
        "your" => {
            let player = recognize_turn_owner(cursor)?;
            StepEventNode {
                step: recognize_step_name(cursor, false)?,
                player,
            }
        }
        "each" => {
            let next = cursor.peek_ahead(1).map(str::to_ascii_lowercase);
            match next.as_deref() {
                Some("of") => {
                    cursor.consume_phrase(&["each", "of"])?;
                    let player = recognize_turn_owner(cursor)?;
                    StepEventNode {
                        step: recognize_step_name(cursor, true)?,
                        player,
                    }
                }
                Some("combat") => {
                    cursor.consume_phrase(&["each", "combat"])?;
                    StepEventNode {
                        step: TurnStep::BeginCombat,
                        player: StepPlayerScope::EachPlayer,
                    }
                }
                Some(raw) if is_possessive(raw, "player") || is_possessive(raw, "opponent") => {
                    let player = recognize_turn_owner(cursor)?;
                    StepEventNode {
                        step: recognize_step_name(cursor, false)?,
                        player,
                    }
                }
                _ => {
                    cursor.consume("each")?;
                    StepEventNode {
                        step: recognize_step_name(cursor, false)?,
                        player: StepPlayerScope::EachPlayer,
                    }
                }
            }
        }
        "the" => {
            cursor.consume("the")?;
            let step = recognize_step_name(cursor, false)?;
            let player = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
            {
                recognize_attached_controller(cursor)?
            } else {
                StepPlayerScope::EachPlayer
            };
            StepEventNode { step, player }
        }
        "combat" => {
            cursor.enter("event.step.combat-turn");
            cursor.consume_phrase(&["combat", "on"])?;
            let player = recognize_turn_owner(cursor)?;
            cursor.enter("event.step.combat-turn.noun");
            cursor.consume("turn")?;
            StepEventNode {
                step: TurnStep::BeginCombat,
                player,
            }
        }
        _ => {
            return Err(cursor.fail("`your`, `each`, `the`, or `combat on TURN-OWNER turn`"));
        }
    };
    cursor.enter("event.step.complete");
    Ok(node)
}
