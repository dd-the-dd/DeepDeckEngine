//! Main-phase ordinals (CR 505.1a) read from a turn's step history.
//!
//! A trigger such as `at the beginning of your second main phase` names the
//! main phase by its position among the main phases of the turn, not by the
//! precombat/postcombat label. The ordinal is the number of main phases the
//! turn's step history has begun up to and including the one at hand.
use super::GameStep;

/// The steps of one turn in the order the turn loop runs them (CR 500.1):
/// beginning phase, precombat main phase, every combat phase, postcombat
/// main phase, ending phase. `combat_phases` counts the regular combat
/// phase plus any additional ones; an additional combat phase adds no main
/// phase here, since no supported effect adds one.
pub(super) fn turn_step_history(combat_phases: usize) -> Vec<GameStep> {
    let mut steps = vec![
        GameStep::Untap,
        GameStep::Upkeep,
        GameStep::Draw,
        GameStep::PrecombatMain,
    ];
    for _ in 0..combat_phases {
        steps.extend([
            GameStep::BeginningOfCombat,
            GameStep::DeclareAttackers,
            GameStep::DeclareBlockers,
            GameStep::CombatDamage,
        ]);
    }
    steps.extend([
        GameStep::PostcombatMain,
        GameStep::EndStep,
        GameStep::Cleanup,
    ]);
    steps
}

/// The IR name of a main step (`stepBegan.step`).
fn main_step(name: &str) -> Option<GameStep> {
    match name {
        "precombatMain" => Some(GameStep::PrecombatMain),
        "postcombatMain" => Some(GameStep::PostcombatMain),
        _ => None,
    }
}

/// The ordinal of the main phase the first occurrence of `step` begins in
/// `history`: the main steps begun up to and including it.
pub(super) fn main_phase_ordinal_in(history: &[GameStep], step: &str) -> Option<u64> {
    let step = main_step(step)?;
    let position = history.iter().position(|candidate| *candidate == step)?;
    let ordinal = history[..=position]
        .iter()
        .filter(|candidate| {
            matches!(
                candidate,
                GameStep::PrecombatMain | GameStep::PostcombatMain
            )
        })
        .count();
    u64::try_from(ordinal).ok()
}

/// The ordinal of the main phase `step` begins in a turn with one combat
/// phase. Additional combat phases never change it, so validation and
/// runtime read the same history.
pub(super) fn main_phase_ordinal_of_step(step: &str) -> Option<u64> {
    main_phase_ordinal_in(&turn_step_history(1), step)
}
