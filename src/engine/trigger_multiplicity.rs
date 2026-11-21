//! Trigger multiplicity: one count implementation and captured source facts.
use super::*;

/// Runtime resource policy, independent of the language's numeric domain.
/// An exceeded budget is an error before allocation, never a capped success.
pub(super) const MAX_TRIGGER_OCCURRENCES_PER_PUBLICATION: usize = 65_536;

/// Semantic provenance supplied by the producer, never guessed from a rule
/// body, its causal card or a live-zone lookup. CR603.2d excludes created
/// delayed/reflexive abilities from abilities an object HAS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TriggerOrigin {
    ObjectAbility,
    Delayed,
    Emblem,
    /// Legacy constructor has no surviving producer proving ability ownership.
    Unproven,
    /// Training infrastructure is explicitly outside Oracle ability semantics.
    Infrastructure,
}

#[derive(Debug)]
pub(super) enum TriggerPublicationOutcome {
    Published(String),
    Suppressed,
    Failed,
}

impl TriggerPublicationOutcome {
    pub(super) fn published_id(&self) -> Option<&str> {
        match self {
            Self::Published(id) => Some(id),
            _ => None,
        }
    }
}

fn checked_trigger_sum(values: impl IntoIterator<Item = i32>) -> Result<i32, EngineError> {
    values.into_iter().try_fold(0, |sum, value| {
        if value < 0 {
            return Err(EngineError::new("negative trigger multiplicity"));
        }
        numeric_evaluation::checked_integer_arithmetic("add", sum, value, None)
            .ok_or_else(|| EngineError::new("trigger multiplicity exceeds its integer bound"))
    })
}

pub(super) fn checked_occurrence_count(additional: i32) -> Result<usize, EngineError> {
    let count = checked_trigger_sum([1, additional])? as usize;
    if count > MAX_TRIGGER_OCCURRENCES_PER_PUBLICATION {
        return Err(EngineError::new(
            "trigger publication resource budget exceeded",
        ));
    }
    Ok(count)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CapturedOccurrence {
    Live,
    Death,
    Sacrifice,
    Departure,
    Destruction,
}

#[derive(Clone, Debug)]
pub(super) struct CapturedTriggerModifier {
    pub(super) modifier: Value,
    pub(super) source_matches: Result<bool, EngineError>,
}

/// Eligible SOURCE facts, not a final count. Event filters are applied only by
/// the publication-time count function with its actual occurrence context.
#[derive(Clone, Debug)]
pub(super) struct TriggerMultiplicitySnapshot {
    pub(super) source_modifiers: Vec<CapturedTriggerModifier>,
    pub(super) legacy_unowned_amounts: Vec<i32>,
    pub(super) source_presence: Result<bool, EngineError>,
    pub(super) windcrag_count: usize,
    pub(super) source_was_attacking: bool,
    pub(super) source_was_creature: bool,
}

#[derive(Clone, Debug)]
pub(super) struct CapturedTriggerSource {
    pub(super) source: CardInstance,
    pub(super) rules: Vec<Value>,
    pub(super) multiplicity: TriggerMultiplicitySnapshot,
}

impl GameEngine {
    fn trigger_source_is_current_permanent(
        &self,
        source: &CardInstance,
    ) -> Result<bool, EngineError> {
        let Some((player, index)) = self.permanent_position(&source.instance_id) else {
            return Ok(false);
        };
        let live = &self.state.players[player].battlefield[index];
        let captured_identity = object_identity::object_identity_of(source)?;
        let live_identity = object_identity::object_identity_of(live)?;
        Ok(captured_identity == live_identity
            && !live.flags.get("phasedOut").copied().unwrap_or(false))
    }

    /// Characteristic evaluation delegates to the existing effective query
    /// service; controller/ownership remain separate relational arguments.
    fn trigger_multiplier_source_matches(
        &self,
        source: &CardInstance,
        selector: &Value,
        candidate: &CardInstance,
        presence: &Result<bool, EngineError>,
    ) -> Result<bool, EngineError> {
        if value_kind(selector) != Some("permanents") {
            if !Self::static_selector_matches(source, selector, candidate) {
                return Ok(false);
            }
            return presence.clone();
        }
        if !Self::static_selector_relations_match(source, selector, candidate)
            || (selector["excludeSource"].as_bool().unwrap_or(false)
                && source.instance_id == candidate.instance_id)
        {
            return Ok(false);
        }
        if !*presence.as_ref().map_err(Clone::clone)? {
            return Ok(false);
        }
        // The color-only preparation role borrows the candidate and world;
        // no stack object, designation, binding or identity is fabricated.
        let prepared =
            self.prepare_static_permanent_object_filter(&selector["where"], candidate)?;
        Ok(self.permanent_matches_effective_filter(candidate, &prepared))
    }

    pub(super) fn capture_trigger_multiplicity_from_modifiers(
        &self,
        source: &CardInstance,
        modifiers: &[(CardInstance, Value)],
    ) -> TriggerMultiplicitySnapshot {
        // Presence belongs to the occurrence capture, not a later lookup.
        // Non-BF abilities are not abilities of a permanent, even if their
        // causal card happens to have permanent card types.
        let has_multiplicity = modifiers.iter().any(|(_, m)| {
            matches!(
                value_kind(m),
                Some("multiplyTriggeredAbility" | "multiplyUnownedControlledTriggeredAbility")
            )
        }) || self
            .state
            .rule_modifiers
            .iter()
            .any(|m| value_kind(m) == Some("windcragSiegeMardu"));
        let presence = if !has_multiplicity {
            Ok(false)
        } else {
            self.trigger_source_is_current_permanent(source)
        };
        let source_modifiers = modifiers
            .iter()
            .filter(|(_, modifier)| value_kind(modifier) == Some("multiplyTriggeredAbility"))
            .map(|(modifier_source, modifier)| CapturedTriggerModifier {
                modifier: modifier.clone(),
                source_matches: self.trigger_multiplier_source_matches(
                    modifier_source,
                    &modifier["sources"],
                    source,
                    &presence,
                ),
            })
            .collect();
        // Historical alias is retained during proof; new Native output never
        // emits it. Removal requires separately verified equivalent behavior.
        let legacy_unowned_amounts = modifiers
            .iter()
            .filter(|(modifier_source, modifier)| {
                value_kind(modifier) == Some("multiplyUnownedControlledTriggeredAbility")
                    && modifier_source.controller == source.controller
                    && source.controller != source.owner
            })
            .filter_map(|(_, modifier)| integer_value(&modifier["additionalTriggers"]))
            .collect::<Vec<_>>();
        let windcrag_count = self
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some("windcragSiegeMardu")
                    && modifier["playerId"].as_str() == Some(source.controller.as_str())
                    && modifier["sourceCardInstanceId"]
                        .as_str()
                        .is_some_and(|id| self.permanent_position(id).is_some())
            })
            .count();
        TriggerMultiplicitySnapshot {
            source_modifiers,
            legacy_unowned_amounts,
            source_presence: presence,
            windcrag_count,
            source_was_attacking: source.flags.get("attacking").copied().unwrap_or(false),
            source_was_creature: self.permanent_is_creature(source),
        }
    }

    pub(super) fn additional_trigger_count(
        &self,
        source: &CardInstance,
        rule: &Value,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<i32, EngineError> {
        let snapshot = self.capture_trigger_multiplicity_from_modifiers(
            source,
            &self.battlefield_static_modifiers(),
        );
        self.additional_trigger_count_from_snapshot(
            source,
            rule,
            decisions,
            &snapshot,
            CapturedOccurrence::Live,
        )
    }

    pub(super) fn additional_trigger_count_from_snapshot(
        &self,
        _source: &CardInstance,
        rule: &Value,
        decisions: &BTreeMap<String, Value>,
        snapshot: &trigger_multiplicity::TriggerMultiplicitySnapshot,
        occurrence: trigger_multiplicity::CapturedOccurrence,
    ) -> Result<i32, EngineError> {
        let mut ordinary = 0;
        for captured in &snapshot.source_modifiers {
            let modifier = &captured.modifier;
            let event_matches = (|| {
                value_kind(modifier) == Some("multiplyTriggeredAbility")
                    && modifier.get("eventWhere").is_none_or(|event_where| {
                        // The admitted eventWhere domain is entry/life gain. A
                        // captured departure occurrence is neither, regardless
                        // of generic triggering IDs or a oneOf declaration.
                        if !matches!(occurrence, trigger_multiplicity::CapturedOccurrence::Live) {
                            return false;
                        }
                        if value_kind(event_where) == Some("permanentEntered")
                            && let Some(triggering_id) = decisions
                                .get("triggeringPermanentId")
                                .and_then(Value::as_str)
                            && let Some((player_index, card_index)) =
                                self.permanent_position(triggering_id)
                        {
                            return matches_card_filter(
                                &self.state.players[player_index].battlefield[card_index]
                                    .definition,
                                &event_where["where"],
                            );
                        }
                        triggered_event_variants(&rule["event"])
                            .into_iter()
                            .any(|event| {
                                value_kind(event) == value_kind(event_where)
                                    && event["where"] == event_where["where"]
                            })
                    })
            })();
            if !event_matches {
                continue;
            }
            if captured.source_matches.as_ref().map_err(Clone::clone)? == &false {
                continue;
            }
            let additional = integer_value(&modifier["additionalTriggers"])
                .ok_or_else(|| EngineError::new("invalid trigger multiplicity amount"))?;
            ordinary = checked_trigger_sum([ordinary, additional])?;
        }
        let unowned_controlled = if !snapshot.legacy_unowned_amounts.is_empty()
            && *snapshot.source_presence.as_ref().map_err(Clone::clone)?
        {
            checked_trigger_sum(snapshot.legacy_unowned_amounts.iter().copied())?
        } else {
            0
        };
        let caused_by_attacker = snapshot.source_was_attacking
            || decisions
                .get("triggeringPermanentId")
                .and_then(Value::as_str)
                .and_then(|instance_id| self.permanent_position(instance_id))
                .is_some_and(|(player_index, card_index)| {
                    self.state.players[player_index].battlefield[card_index]
                        .flags
                        .get("attacking")
                        .copied()
                        .unwrap_or(false)
                })
            || triggered_event_variants(&rule["event"])
                .iter()
                .any(|event| {
                    matches!(
                        value_kind(event),
                        Some("declaredAttacker" | "controlledCreaturesAttacked")
                    )
                })
            || super::event_model::rule_combat_patterns(rule)
                .iter()
                .any(|pattern| {
                    pattern.generic && pattern.lemma == super::event_model::CombatLemma::Attack
                });
        let windcrag = if caused_by_attacker
            && snapshot.windcrag_count > 0
            && *snapshot.source_presence.as_ref().map_err(Clone::clone)?
        {
            i32::try_from(snapshot.windcrag_count)
                .map_err(|_| EngineError::new("trigger multiplicity exceeds its integer bound"))?
        } else {
            0
        };
        checked_trigger_sum([ordinary, unowned_controlled, windcrag])
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "trigger_multiplicity_tests.rs"]
mod tests;
