use super::*;

fn identity(marker: &str) -> BattlefieldTargetIdentity {
    BattlefieldTargetIdentity {
        instance_id: "same-physical-card".into(),
        zone_identity: marker.into(),
    }
}
fn values(mana: i32, power: i32, toughness: i32) -> TargetNumericValues {
    TargetNumericValues {
        mana_value: mana,
        power: Some(power),
        toughness: Some(toughness),
    }
}
fn context() -> TargetNumericContext {
    TargetNumericContext {
        stack_id: "stack:original".into(),
        legal_at_start: BTreeSet::from(["targetPermanent".into()]),
        known_targets_at_start: BTreeSet::from(["targetPermanent".into()]),
        unselected_at_start: BTreeSet::new(),
        unauthenticated_at_start: BTreeSet::new(),
        targets: BTreeMap::from([(
            "targetPermanent".into(),
            NumericTargetBinding {
                identity: identity("battlefield:original"),
                last_known: None,
            },
        )]),
    }
}

#[test]
fn live_modifier_and_failed_destruction_read_current_properties() {
    let binding = context().targets.remove("targetPermanent").unwrap();
    let current = values(5, 4, 5);
    assert_eq!(
        read_target_numeric(
            &binding,
            Some((&binding.identity, &current)),
            TargetNumericProperty::Power
        ),
        Ok(4)
    );
    assert_eq!(
        read_target_numeric(
            &binding,
            Some((&binding.identity, &current)),
            TargetNumericProperty::Toughness
        ),
        Ok(5)
    );
    assert_eq!(
        read_target_numeric(
            &binding,
            Some((&binding.identity, &current)),
            TargetNumericProperty::ManaValue
        ),
        Ok(5)
    );
}

#[test]
fn shield_survivor_never_publishes_a_prepared_snapshot() {
    let binding = context().targets.remove("targetPermanent").unwrap();
    let _not_committed = PreparedTargetDeparture {
        identity: binding.identity.clone(),
        values: values(5, 2, 3),
    };
    let after_shield = values(5, 4, 5);
    assert_eq!(
        read_target_numeric(
            &binding,
            Some((&binding.identity, &after_shield)),
            TargetNumericProperty::Power
        ),
        Ok(4)
    );
    assert!(binding.last_known.is_none());
}

#[test]
fn departed_copy_uses_effective_snapshot_and_never_printed_graveyard_values() {
    let mut context = context();
    let prepared = PreparedTargetDeparture {
        identity: identity("battlefield:original"),
        values: values(5, 6, 7),
    };
    context.confirm_departure(&prepared);
    let binding = &context.targets["targetPermanent"];
    assert_eq!(
        read_target_numeric(binding, None, TargetNumericProperty::ManaValue),
        Ok(5)
    );
    assert_eq!(
        read_target_numeric(binding, None, TargetNumericProperty::Power),
        Ok(6)
    );
    assert_eq!(
        read_target_numeric(binding, None, TargetNumericProperty::Toughness),
        Ok(7)
    );
}

#[test]
fn blink_return_with_same_physical_id_never_replaces_original_incarnation() {
    let mut context = context();
    context.confirm_departure(&PreparedTargetDeparture {
        identity: identity("battlefield:original"),
        values: values(5, 2, 3),
    });
    let returned = identity("battlefield:returned");
    assert_eq!(
        read_target_numeric(
            &context.targets["targetPermanent"],
            Some((&returned, &values(9, 10, 11))),
            TargetNumericProperty::ManaValue
        ),
        Ok(5)
    );
    context.confirm_departure(&PreparedTargetDeparture {
        identity: returned,
        values: values(9, 10, 11),
    });
    assert_eq!(
        context.targets["targetPermanent"].last_known,
        Some(values(5, 2, 3))
    );
}

#[test]
fn missing_snapshot_is_an_error_while_legitimate_zero_is_a_value() {
    let binding = context().targets.remove("targetPermanent").unwrap();
    assert_eq!(
        read_target_numeric(&binding, None, TargetNumericProperty::ManaValue),
        Err(TargetNumericReadError::MissingLastKnown)
    );
    assert_eq!(
        read_target_numeric(
            &binding,
            Some((&binding.identity, &values(0, 0, 0))),
            TargetNumericProperty::ManaValue
        ),
        Ok(0)
    );
}

#[test]
fn nested_scopes_reuse_same_original_context_without_resetting_lki() {
    let mut frames = TargetNumericContexts::default();
    let outer = frames.begin(context()).unwrap();
    frames.confirm_departure(&PreparedTargetDeparture {
        identity: identity("battlefield:original"),
        values: values(5, 2, 3),
    });
    let nested = frames.begin(context()).unwrap();
    assert_eq!(
        frames
            .binding("stack:original", "targetPermanent")
            .unwrap()
            .last_known,
        Some(values(5, 2, 3))
    );
    frames.end(nested).unwrap();
    frames.end(outer).unwrap();
    assert_eq!(frames.binding("stack:original", "targetPermanent"), None);
}

#[test]
fn checkpoint_clone_restores_active_snapshot_with_state_rollback() {
    let mut frames = TargetNumericContexts::default();
    frames.begin(context()).unwrap();
    let checkpoint = frames.clone();
    frames.confirm_departure(&PreparedTargetDeparture {
        identity: identity("battlefield:original"),
        values: values(5, 2, 3),
    });
    frames = checkpoint;
    assert_eq!(
        frames
            .binding("stack:original", "targetPermanent")
            .unwrap()
            .last_known,
        None
    );
}

#[test]
fn reentry_through_a_different_nested_stack_keeps_the_original_snapshot() {
    let mut frames = TargetNumericContexts::default();
    let original = frames.begin(context()).unwrap();
    frames.confirm_departure(&PreparedTargetDeparture {
        identity: identity("battlefield:original"),
        values: values(5, 2, 3),
    });
    let mut other = context();
    other.stack_id = "stack:nested".into();
    let nested = frames.begin(other).unwrap();
    let reentry = frames.begin(context()).unwrap();
    assert_eq!(
        frames
            .binding("stack:original", "targetPermanent")
            .unwrap()
            .last_known,
        Some(values(5, 2, 3))
    );
    frames.end(reentry).unwrap();
    frames.end(nested).unwrap();
    frames.end(original).unwrap();
}

#[test]
fn scoped_player_legality_is_separate_from_battlefield_numeric_lki() {
    let mut original = context();
    original
        .known_targets_at_start
        .extend(["illegalPlayer".into(), "legalPlayer".into()]);
    original.legal_at_start.insert("legalPlayer".into());
    original
        .known_targets_at_start
        .insert("unselectedOptional".into());
    original
        .unselected_at_start
        .insert("unselectedOptional".into());
    let mut frames = TargetNumericContexts::default();
    let scope = frames.begin(original.clone()).unwrap();
    assert_eq!(
        frames.availability_at_start("stack:original", "unselectedOptional"),
        Some(ResolutionTargetAvailability::Unselected)
    );
    assert_eq!(
        frames.availability_at_start("stack:original", "illegalPlayer"),
        Some(ResolutionTargetAvailability::Illegal)
    );
    assert_eq!(
        frames.legal_at_start("stack:original", "unselectedOptional"),
        Some(false)
    );
    assert_eq!(
        frames.legal_at_start("stack:original", "illegalPlayer"),
        Some(false)
    );
    assert_eq!(
        frames.legal_at_start("stack:original", "legalPlayer"),
        Some(true)
    );
    assert_eq!(
        frames.legal_at_start("stack:original", "missingPlayer"),
        None
    );
    assert_eq!(frames.binding("stack:original", "legalPlayer"), None);
    original.legal_at_start.remove("legalPlayer");
    let nested = frames.begin(original).unwrap();
    assert_eq!(
        frames.legal_at_start("stack:original", "legalPlayer"),
        Some(true)
    );
    frames.end(nested).unwrap();
    frames.end(scope).unwrap();
    assert_eq!(frames.legal_at_start("stack:original", "legalPlayer"), None);
}
