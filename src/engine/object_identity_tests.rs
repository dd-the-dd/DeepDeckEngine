//! Prospective identity-kernel tests, not a proof of live initializer closure.
use super::object_identity::{
    IdentityAssignment, ObjectIdentity, install_object_identities, prepare_object_identities,
};
use super::*;

fn carrier(id: &str) -> CardInstance {
    let definition = CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: "Artifact".into(),
        mana_cost: String::new(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        rules: Vec::new(),
    };
    card_instance("p0", &definition, 0)
}

#[test]
fn identity_reservation_is_readonly_and_admission_is_unique() {
    let mut first = carrier("first");
    let mut second = carrier("second");
    let before = (first.clone(), second.clone());
    let mut clock = 12;
    let prepared =
        prepare_object_identities(clock, &[&first, &second], IdentityAssignment::Admission)
            .unwrap();
    assert_eq!((&first, &second), (&before.0, &before.1));
    assert_eq!(clock, 12);
    let assigned =
        install_object_identities(&mut clock, &mut [&mut first, &mut second], prepared).unwrap();
    assert_eq!(clock, 14);
    assert_ne!(assigned[0], assigned[1]);
    assert_eq!(
        GameEngine::tracked_zone_identity(&first),
        Some(assigned[0].zone_identity.as_str())
    );
    assert_eq!(
        GameEngine::tracked_zone_identity(&second),
        Some(assigned[1].zone_identity.as_str())
    );
}

#[test]
fn identity_reservation_overflow_and_duplicate_refuse_before_mutation() {
    let first = carrier("same");
    let second = carrier("same");
    let before = first.clone();
    assert!(
        prepare_object_identities(usize::MAX, &[&first], IdentityAssignment::Admission).is_err()
    );
    assert!(
        prepare_object_identities(0, &[&first, &second], IdentityAssignment::Admission).is_err()
    );
    assert_eq!(first, before);
}

#[test]
fn stale_clock_or_carrier_refuses_entire_batch_without_partial_write() {
    let mut first = carrier("first");
    let mut second = carrier("second");
    let plan =
        prepare_object_identities(0, &[&first, &second], IdentityAssignment::Admission).unwrap();
    GameEngine::assign_tracked_zone_identity(&mut second, "historical-battlefield-marker");
    let before = (first.clone(), second.clone());
    let mut clock = 0;
    assert!(install_object_identities(&mut clock, &mut [&mut first, &mut second], plan).is_err());
    assert_eq!((&first, &second), (&before.0, &before.1));
    assert_eq!(clock, 0);
    let mut fresh = carrier("third");
    let plan = prepare_object_identities(0, &[&fresh], IdentityAssignment::Admission).unwrap();
    clock = 1;
    assert!(install_object_identities(&mut clock, &mut [&mut fresh], plan).is_err());
    assert_eq!(clock, 1);
    assert!(GameEngine::tracked_zone_identity(&fresh).is_none());
}

#[test]
fn new_incarnation_preserves_physical_id_and_invalidates_old_identity() {
    let mut card = carrier("same-physical-card");
    GameEngine::assign_tracked_zone_identity(&mut card, "battlefield:incarnation:21:old");
    let old = ObjectIdentity {
        instance_id: card.instance_id.clone(),
        zone_identity: "battlefield:incarnation:21:old".into(),
    };
    let snapshot = card.clone();
    let mut clock = 21;
    let plan =
        prepare_object_identities(clock, &[&card], IdentityAssignment::NewIncarnation).unwrap();
    let identities = install_object_identities(&mut clock, &mut [&mut card], plan).unwrap();
    assert_eq!(identities[0].instance_id, old.instance_id);
    assert_ne!(identities[0], old);
    assert_eq!(
        GameEngine::tracked_zone_identity(&snapshot),
        Some(old.zone_identity.as_str())
    );
    assert_eq!(clock, 22);
}

#[test]
fn admission_never_repairs_tracked_objects_and_transition_never_repairs_missing_identity() {
    let mut card = carrier("tracked");
    assert!(prepare_object_identities(0, &[&card], IdentityAssignment::NewIncarnation).is_err());
    GameEngine::assign_tracked_zone_identity(&mut card, "historical-exile-marker");
    assert!(prepare_object_identities(0, &[&card], IdentityAssignment::Admission).is_err());
}

#[test]
fn identity_serialization_preserves_historical_two_field_shape() {
    let captured = ObjectIdentity {
        instance_id: "physical".into(),
        zone_identity: "battlefield:incarnation:7:physical".into(),
    };
    let wire = serde_json::to_value(&captured).unwrap();
    assert_eq!(
        wire,
        json!({"instanceId":"physical","zoneIdentity":"battlefield:incarnation:7:physical"})
    );
    assert_eq!(
        serde_json::from_value::<ObjectIdentity>(wire).unwrap(),
        captured
    );
}
