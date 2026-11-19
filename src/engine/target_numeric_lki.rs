//! Proposed identity-bound numeric LKI core. Staging only; not integrated.
//! Declaration records identity. Only an actual departure records values.
use std::collections::{BTreeMap, BTreeSet};

/// Compatibility name for the shared object identity; numeric LKI remains BF-only.
pub(super) type BattlefieldTargetIdentity = super::object_identity::ObjectIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetNumericProperty {
    ManaValue,
    Power,
    Toughness,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TargetNumericValues {
    pub mana_value: i32,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
}

impl TargetNumericValues {
    pub(super) fn read(
        &self,
        property: TargetNumericProperty,
    ) -> Result<i32, TargetNumericReadError> {
        match property {
            TargetNumericProperty::ManaValue => Ok(self.mana_value),
            TargetNumericProperty::Power => {
                self.power.ok_or(TargetNumericReadError::PropertyNotDefined)
            }
            TargetNumericProperty::Toughness => self
                .toughness
                .ok_or(TargetNumericReadError::PropertyNotDefined),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NumericTargetBinding {
    pub identity: BattlefieldTargetIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known: Option<TargetNumericValues>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetNumericReadError {
    MissingDesignation,
    MissingIncarnation,
    MissingLastKnown,
    PropertyNotDefined,
    ConflictingContext,
    ContextOrder,
    MalformedCapture,
    UnsupportedTargetKind,
    IllegalAtResolutionStart,
    MissingResolutionContext,
}

/// The caller supplies the current object only after obtaining its actual
/// tracked identity. Physical IDs alone are insufficient. A matching current
/// object always wins over LKI (failed destruction, shield, live modifier).
pub(super) fn read_target_numeric(
    binding: &NumericTargetBinding,
    current: Option<(&BattlefieldTargetIdentity, &TargetNumericValues)>,
    property: TargetNumericProperty,
) -> Result<i32, TargetNumericReadError> {
    if let Some((identity, values)) = current
        && identity == &binding.identity
    {
        return values.read(property);
    }
    binding
        .last_known
        .as_ref()
        .ok_or(TargetNumericReadError::MissingLastKnown)?
        .read(property)
}

/// Values can be frozen for a simultaneous group before its first removal.
/// Preparing this record does NOT publish LKI: only confirm_departure commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedTargetDeparture {
    pub identity: BattlefieldTargetIdentity,
    pub values: TargetNumericValues,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResolutionTargetAvailability {
    Legal,
    Illegal,
    Unselected,
    Unauthenticated,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct TargetNumericContext {
    pub stack_id: String,
    pub targets: BTreeMap<String, NumericTargetBinding>,
    pub legal_at_start: BTreeSet<String>,
    pub known_targets_at_start: BTreeSet<String>,
    pub unselected_at_start: BTreeSet<String>,
    pub unauthenticated_at_start: BTreeSet<String>,
}

impl TargetNumericContext {
    pub(super) fn confirm_departure(&mut self, departed: &PreparedTargetDeparture) {
        for binding in self.targets.values_mut() {
            if binding.identity == departed.identity && binding.last_known.is_none() {
                binding.last_known = Some(departed.values.clone());
            }
        }
    }
}

/// Private GameEngine state, cloned with engine-wide checkpoints. Partial
/// GameState-only payment rollback MUST also restore this field explicitly.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct TargetNumericContexts {
    frames: Vec<TargetNumericContext>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TargetNumericScope {
    depth: usize,
    owns_frame: bool,
}

impl TargetNumericContexts {
    pub(super) fn availability_at_start(
        &self,
        stack_id: &str,
        target_id: &str,
    ) -> Option<ResolutionTargetAvailability> {
        let frame = self
            .frames
            .iter()
            .rev()
            .find(|frame| frame.stack_id == stack_id)?;
        if !frame.known_targets_at_start.contains(target_id) {
            return None;
        }
        Some(if frame.unauthenticated_at_start.contains(target_id) {
            ResolutionTargetAvailability::Unauthenticated
        } else if frame.unselected_at_start.contains(target_id) {
            ResolutionTargetAvailability::Unselected
        } else if frame.legal_at_start.contains(target_id) {
            ResolutionTargetAvailability::Legal
        } else {
            ResolutionTargetAvailability::Illegal
        })
    }
    pub(super) fn legal_at_start(&self, stack_id: &str, target_id: &str) -> Option<bool> {
        self.availability_at_start(stack_id, target_id)
            .and_then(|status| match status {
                ResolutionTargetAvailability::Unauthenticated => None,
                _ => Some(status == ResolutionTargetAvailability::Legal),
            })
    }
    pub(super) fn observes(&self, identity: &BattlefieldTargetIdentity) -> bool {
        self.frames.iter().any(|frame| {
            frame
                .targets
                .values()
                .any(|binding| &binding.identity == identity)
        })
    }
    pub(super) fn begin(
        &mut self,
        context: TargetNumericContext,
    ) -> Result<TargetNumericScope, TargetNumericReadError> {
        if let Some(current) = self
            .frames
            .iter()
            .rev()
            .find(|frame| frame.stack_id == context.stack_id)
        {
            // Reentrant effects of the same stack object reuse its original
            // frame and accumulated departure records; never reset the LKI.
            let same_designations = current.known_targets_at_start
                == context.known_targets_at_start
                && current.targets.len() == context.targets.len()
                && current.targets.iter().all(|(id, binding)| {
                    context
                        .targets
                        .get(id)
                        .is_some_and(|other| other.identity == binding.identity)
                });
            if !same_designations {
                return Err(TargetNumericReadError::ConflictingContext);
            }
            return Ok(TargetNumericScope {
                depth: self.frames.len(),
                owns_frame: false,
            });
        }
        self.frames.push(context);
        Ok(TargetNumericScope {
            depth: self.frames.len(),
            owns_frame: true,
        })
    }

    pub(super) fn end(&mut self, scope: TargetNumericScope) -> Result<(), TargetNumericReadError> {
        if self.frames.len() != scope.depth {
            return Err(TargetNumericReadError::ContextOrder);
        }
        if scope.owns_frame {
            self.frames.pop();
        }
        Ok(())
    }

    pub(super) fn confirm_departure(&mut self, departed: &PreparedTargetDeparture) {
        for frame in &mut self.frames {
            frame.confirm_departure(departed);
        }
    }

    pub(super) fn binding(&self, stack_id: &str, target_id: &str) -> Option<&NumericTargetBinding> {
        self.frames
            .iter()
            .rev()
            .find(|frame| frame.stack_id == stack_id)?
            .targets
            .get(target_id)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4")
))]
#[path = "target_numeric_lki_tests.rs"]
mod tests;
