use super::{
    FrozenGraveyardDestination, PlannedDestructionOutcome, exactly_one_or_none,
    validate_destruction_replacement_choice, validate_zone_or_death_replacement_choice,
};
use crate::engine::EngineError;

#[test]
fn graveyard_destination_replacement_preserves_destroyed_but_not_died() {
    assert!(
        PlannedDestructionOutcome::MoveTo(FrozenGraveyardDestination::Graveyard)
            .counts_as_destroyed()
    );
    assert!(
        PlannedDestructionOutcome::MoveTo(FrozenGraveyardDestination::Exile {
            replacement_source_id: "replacement".to_string(),
            replacement_controller_id: Some("player-0".to_string()),
        })
        .counts_as_destroyed()
    );
    assert!(PlannedDestructionOutcome::LeavingBattlefieldReplacedWithExile.counts_as_destroyed());
    assert!(
        PlannedDestructionOutcome::DeathReplacedByRuleModifier {
            modifier_kind: "exileIfWouldDie".to_string(),
        }
        .counts_as_destroyed()
    );
    assert!(!PlannedDestructionOutcome::Indestructible.counts_as_destroyed());
    assert!(!PlannedDestructionOutcome::Regenerated.counts_as_destroyed());
    assert!(!PlannedDestructionOutcome::ShieldCounter.counts_as_destroyed());
}

#[test]
fn ambiguous_replacements_fail_before_commit() {
    let result = exactly_one_or_none(vec!["first", "second"], || {
        EngineError::new("ambiguous replacement")
    });

    assert!(result.is_err());
}

#[test]
fn competing_destruction_replacement_families_fail_before_commit() {
    assert!(validate_destruction_replacement_choice("target", true, true, false).is_err());
    assert!(validate_destruction_replacement_choice("target", true, false, true).is_err());
    assert!(validate_destruction_replacement_choice("target", false, true, true).is_err());
    assert!(validate_destruction_replacement_choice("target", true, false, false).is_ok());
}

#[test]
fn competing_zone_and_death_replacement_families_fail_before_commit() {
    assert!(validate_zone_or_death_replacement_choice("target", true, false, false, true).is_err());
    assert!(validate_zone_or_death_replacement_choice("target", false, true, false, true).is_err());
    assert!(validate_zone_or_death_replacement_choice("target", true, false, true, false).is_err());
    assert!(validate_zone_or_death_replacement_choice("target", false, false, false, true).is_ok());
}
