//! Checked identity reservations; no query or declaration can mint an identity.
//! Staging only: not registered until initializers and movement writers close.

use std::collections::BTreeSet;

use super::{CardInstance, EngineError, GameEngine};

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ObjectIdentity {
    pub instance_id: String,
    pub zone_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IdentityAssignment {
    /// A new carrier entering live state, before it can be referenced.
    Admission,
    /// A committed transition/new-object instruction on an existing object.
    NewIncarnation,
}

/// Linear reservation. Constructing it changes neither carriers nor the clock.
#[derive(Debug)]
pub(super) struct PreparedObjectIdentities {
    previous_clock: usize,
    next_clock: usize,
    carriers: Vec<(String, Option<String>, ObjectIdentity)>,
}

pub(super) fn prepare_object_identities(
    clock: usize,
    cards: &[&CardInstance],
    assignment: IdentityAssignment,
) -> Result<PreparedObjectIdentities, EngineError> {
    let requests = cards
        .iter()
        .map(|card| (assignment, *card))
        .collect::<Vec<_>>();
    prepare_object_identity_requests(clock, &requests)
}

pub(super) fn prepare_object_identity_requests(
    clock: usize,
    cards: &[(IdentityAssignment, &CardInstance)],
) -> Result<PreparedObjectIdentities, EngineError> {
    let next_clock = clock
        .checked_add(cards.len())
        .ok_or_else(|| EngineError::new("object incarnation sequence exhausted"))?;
    let mut seen = BTreeSet::new();
    let mut carriers = Vec::with_capacity(cards.len());
    for (index, (assignment, card)) in cards.iter().enumerate() {
        if card.instance_id.is_empty() || !seen.insert(card.instance_id.as_str()) {
            return Err(EngineError::new(
                "object identity requires unique nonempty carrier IDs",
            ));
        }
        let previous = existing_object_identity_of(card)?.map(|identity| identity.zone_identity);
        match (*assignment, previous.as_deref()) {
            (IdentityAssignment::Admission, None) => {}
            (IdentityAssignment::NewIncarnation, Some(identity)) if !identity.is_empty() => {}
            _ => {
                return Err(EngineError::new(
                    "object identity assignment has invalid admission state",
                ));
            }
        }
        // checked_add(cards.len()) proved this entire reservation fits.
        let serial = clock + index + 1;
        let identity = ObjectIdentity {
            instance_id: card.instance_id.clone(),
            zone_identity: format!("object:incarnation:{serial}:{}", card.instance_id),
        };
        carriers.push((card.instance_id.clone(), previous, identity));
    }
    Ok(PreparedObjectIdentities {
        previous_clock: clock,
        next_clock,
        carriers,
    })
}

/// All validation precedes the first marker write. This assigns identities only;
/// it is not permission to move cards or publish any movement observation.
pub(super) fn install_object_identities(
    clock: &mut usize,
    cards: &mut [&mut CardInstance],
    prepared: PreparedObjectIdentities,
) -> Result<Vec<ObjectIdentity>, EngineError> {
    if *clock != prepared.previous_clock || cards.len() != prepared.carriers.len() {
        return Err(EngineError::new("stale object identity reservation"));
    }
    for (card, (instance_id, previous, _)) in cards.iter().zip(&prepared.carriers) {
        if &card.instance_id != instance_id
            || existing_object_identity_of(card)?
                .as_ref()
                .map(|identity| identity.zone_identity.as_str())
                != previous.as_deref()
        {
            return Err(EngineError::new(
                "object identity carrier changed before installation",
            ));
        }
    }
    let mut installed = Vec::with_capacity(cards.len());
    for (card, (_, _, identity)) in cards.iter_mut().zip(prepared.carriers) {
        GameEngine::assign_tracked_zone_identity(card, &identity.zone_identity);
        installed.push(identity);
    }
    *clock = prepared.next_clock;
    Ok(installed)
}

/// Strict marker validation shared by admission, allocation and capture.
pub(super) fn existing_object_identity_of(
    card: &CardInstance,
) -> Result<Option<ObjectIdentity>, EngineError> {
    use super::zone_change_triggers::TRACKED_ZONE_PREFIX;
    if card.instance_id.is_empty()
        || card
            .flags
            .iter()
            .filter(|(key, active)| **active && key.starts_with(TRACKED_ZONE_PREFIX))
            .count()
            > 1
    {
        return Err(EngineError::new(
            "object has an invalid or ambiguous incarnation",
        ));
    }
    let Some(zone_identity) = GameEngine::tracked_zone_identity(card) else {
        return Ok(None);
    };
    if zone_identity.is_empty() {
        return Err(EngineError::new("object has an empty incarnation"));
    }
    Ok(Some(ObjectIdentity {
        instance_id: card.instance_id.clone(),
        zone_identity: zone_identity.into(),
    }))
}

/// Capture only an identity already established at admission/commit.
pub(super) fn object_identity_of(card: &CardInstance) -> Result<ObjectIdentity, EngineError> {
    existing_object_identity_of(card)?
        .ok_or_else(|| EngineError::new("object has no admitted incarnation"))
}

impl GameEngine {
    pub(super) fn assign_new_object_incarnation(
        &mut self,
        card: &mut CardInstance,
    ) -> Result<(), EngineError> {
        let assignment = if Self::tracked_zone_identity(card).is_some() {
            IdentityAssignment::NewIncarnation
        } else {
            IdentityAssignment::Admission
        };
        let prepared = prepare_object_identities(
            self.state.battlefield_incarnation_sequence,
            &[card],
            assignment,
        )?;
        install_object_identities(
            &mut self.state.battlefield_incarnation_sequence,
            &mut [card],
            prepared,
        )?;
        Ok(())
    }
}
