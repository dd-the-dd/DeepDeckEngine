//! Authenticated scalar subjects shared by ordinary permanent actions.
use super::*;

pub(super) enum PermanentSubjectResolution {
    Historical,
    Unavailable,
    Resolved(Vec<String>),
}

pub(super) fn node_requires_checkpoint(node: &Value) -> bool {
    matches!(
        value_kind(node),
        Some("putCounters" | "grantKeyword" | "modifyPowerToughness" | "untapPermanent")
    )
}

impl GameEngine {
    pub(super) fn prepare_permanent_action_subject(
        &self,
        effect: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<PermanentSubjectResolution, EngineError> {
        let (subject, parameters): (&Value, Vec<&Value>) = match value_kind(effect) {
            Some("putCounters") => (
                &effect["permanent"],
                vec![&effect["count"], &effect["placedBy"]],
            ),
            Some("grantKeyword") => (&effect["object"], vec![&effect["duration"]]),
            Some("modifyPowerToughness") => (
                &effect["object"],
                vec![&effect["power"], &effect["toughness"], &effect["duration"]],
            ),
            Some("untapPermanent") => (&effect["permanent"], Vec::new()),
            _ => {
                return Err(EngineError::new(
                    "unsupported permanent action subject preparation",
                ));
            }
        };
        let resolved = self.resolve_permanent_action_subject(subject, source)?;
        if matches!(resolved, PermanentSubjectResolution::Unavailable) {
            return Ok(resolved);
        }
        // This is a shallow list of evaluated operands. The accepted shared
        // dependency walker owns recursion/conditional-value branch selection.
        for argument in std::iter::once(subject).chain(parameters) {
            if self.value_requires_initially_illegal_target_in_context(
                argument, source, bindings, decisions,
            )? {
                return Ok(PermanentSubjectResolution::Unavailable);
            }
        }
        Ok(resolved)
    }

    pub(super) fn permanent_action_expiry_turn(
        &self,
        duration: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Option<u32>, EngineError> {
        if value_kind(duration) != Some("untilNextTurn") {
            return Ok(None);
        }
        let player = self
            .resolve_player(&duration["player"], source, bindings)
            .ok_or_else(|| EngineError::new("permanent action duration owner is unresolved"))?;
        self.player_index(&player)?;
        self.next_turn_number_for_player(&player)
            .map(Some)
            .ok_or_else(|| {
                EngineError::new("permanent action duration owner has no supported next turn")
            })
    }

    /// Absence/new incarnation is a rules no-op; missing authenticated context
    /// is an error. Existing collection and historical reference paths remain
    /// owned by their original domains.
    pub(super) fn resolve_permanent_action_subject(
        &self,
        reference: &Value,
        source: &StackObject,
    ) -> Result<PermanentSubjectResolution, EngineError> {
        let live_id = match value_kind(reference) {
            Some("observedEventObject") => {
                self.resolve_observed_event_permanent(reference, source)?
            }
            Some("self" | "abilitySource") => {
                let captured = object_identity::object_identity_of(&source.card)?;
                if source_lki::source_has_departed(source) {
                    return Ok(PermanentSubjectResolution::Unavailable);
                }
                let live = self
                    .permanent_position(&source.card.instance_id)
                    .map(|(player, index)| &self.state.players[player].battlefield[index]);
                if let Some(live) = live {
                    let current = object_identity::object_identity_of(live)?;
                    if current != captured {
                        return Ok(PermanentSubjectResolution::Unavailable);
                    }
                }
                // Both sides, when present, are authenticated before the
                // historical SourceLKI chooser observes marker equality.
                let input = source_lki::source_object_input(source, live);
                (input.on_battlefield
                    && !input.card.flags.get("phasedOut").copied().unwrap_or(false))
                .then(|| input.card.instance_id.clone())
            }
            Some("chosenTarget") => {
                if self.value_requires_initially_illegal_target(reference, source)? {
                    return Ok(PermanentSubjectResolution::Unavailable);
                }
                let id = reference["id"].as_str().ok_or_else(|| {
                    EngineError::new("permanent subject requires its original target slot")
                })?;
                let target = source.targets.get(id).ok_or_else(|| {
                    EngineError::new("permanent subject target designation is missing")
                })?;
                let TargetRef::Permanent { instance_id } = target else {
                    return Err(EngineError::new(
                        "permanent action subject has a nonpermanent target designation",
                    ));
                };
                if let Some((player, index)) = self.permanent_position(instance_id) {
                    object_identity::object_identity_of(
                        &self.state.players[player].battlefield[index],
                    )?;
                }
                (self.target_incarnation_is_current(source, id, target)
                    && self
                        .permanent_position(instance_id)
                        .is_some_and(|(player, index)| {
                            !self.state.players[player].battlefield[index]
                                .flags
                                .get("phasedOut")
                                .copied()
                                .unwrap_or(false)
                        }))
                .then(|| instance_id.clone())
            }
            Some("attachedPermanent") if value_kind(&reference["attachment"]) == Some("self") => {
                // CR 301.5a, 303.4a: the permanent the source is attached to
                // now; once the source left, its last known attachment.
                let attached = self
                    .permanent_position(&source.card.instance_id)
                    .map(|(player, index)| {
                        self.state.players[player].battlefield[index]
                            .attached_to
                            .clone()
                    })
                    .unwrap_or_else(|| source.card.attached_to.clone());
                attached.filter(|instance_id| {
                    self.permanent_position(instance_id)
                        .is_some_and(|(player, index)| {
                            !self.state.players[player].battlefield[index]
                                .flags
                                .get("phasedOut")
                                .copied()
                                .unwrap_or(false)
                        })
                })
            }
            _ => return Ok(PermanentSubjectResolution::Historical),
        };
        Ok(match live_id {
            Some(id) => PermanentSubjectResolution::Resolved(vec![id]),
            None => PermanentSubjectResolution::Unavailable,
        })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "permanent_subject_tests.rs"]
mod tests;
